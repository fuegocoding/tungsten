//! SQLite-backed persistence for [`NoteIndex`].
//!
//! The M1.2 design (PRD §4.9) calls for a sidecar SQLite DB at
//! `vault/.tungsten/index.db` so the index survives app restarts
//! and a vault with many notes doesn't pay the parse cost on every
//! launch.
//!
//! **On-disk schema (v1):**
//!
//! ```sql
//! CREATE TABLE notes (
//!     path          TEXT PRIMARY KEY,
//!     title         TEXT NOT NULL,
//!     title_lower   TEXT NOT NULL,
//!     content       TEXT NOT NULL,
//!     frontmatter   TEXT,        -- YAML, or NULL
//!     mtime_secs    INTEGER,     -- unix seconds, or NULL
//!     size_bytes    INTEGER NOT NULL,
//!     tags_csv      TEXT NOT NULL DEFAULT ''  -- comma-separated
//! );
//! CREATE INDEX notes_title_lower_idx ON notes(title_lower);
//! CREATE TABLE tags (
//!     tag         TEXT NOT NULL,
//!     note_path   TEXT NOT NULL,
//!     PRIMARY KEY (tag, note_path)
//! );
//! CREATE INDEX tags_tag_idx ON tags(tag);
//! CREATE TABLE links (
//!     source_path  TEXT NOT NULL,
//!     target       TEXT NOT NULL,        -- already lowercased
//!     alias        TEXT,                 -- nullable
//!     kind         TEXT NOT NULL,        -- 'wiki' | 'wiki-heading' | 'wiki-block' | 'wiki-section' | 'markdown'
//!     byte_start   INTEGER NOT NULL,
//!     byte_end     INTEGER NOT NULL,
//!     PRIMARY KEY (source_path, byte_start)
//! );
//! CREATE INDEX links_target_idx ON links(target);
//! CREATE TABLE meta (
//!     key   TEXT PRIMARY KEY,
//!     value TEXT NOT NULL
//! );
//! ```
//!
//! **Strategy:** a full rewrite on every save. Incremental updates
//! (write a single note on `update_note`) are a follow-up. For
//! vaults up to ~10k notes the full-rewrite path is fast enough
//! that we don't need row-level diffing yet.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::index::NoteIndex;
use crate::note::{Link, LinkKind, Note};
use std::time::{Duration, UNIX_EPOCH};

/// Current schema version. Bumped on any breaking schema change.
pub const SCHEMA_VERSION: i32 = 1;

/// Errors from the SQLite persistence layer.
#[derive(Debug, thiserror::Error)]
pub enum IndexDbError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("schema version mismatch: db has {db}, code expects {expected}")]
    SchemaVersion { db: i32, expected: i32 },
    #[error("malformed row in table {table}: {message}")]
    Malformed { table: String, message: String },
}

impl NoteIndex {
    /// Persist this index to a SQLite database at `db_path`. Creates
    /// the file (and parent directories) if needed. The whole index
    /// is rewritten in a single transaction.
    pub fn save_to_sqlite(&self, db_path: &Path) -> Result<(), IndexDbError> {
        if let Some(parent) = db_path.parent() {
            if !parent.is_dir() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut conn = Connection::open(db_path)?;
        init_schema(&mut conn)?;
        let tx = conn.transaction()?;
        // Clear existing rows.
        tx.execute("DELETE FROM links", [])?;
        tx.execute("DELETE FROM tags", [])?;
        tx.execute("DELETE FROM notes", [])?;
        // Write notes.
        {
            let mut stmt = tx.prepare(
                "INSERT INTO notes (path, title, title_lower, content, frontmatter, mtime_secs, size_bytes, tags_csv)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for note in self.notes() {
                let path = note.path.to_string_lossy().to_string();
                let title = &note.title;
                let title_lower = title.to_lowercase();
                let content = &note.content;
                let frontmatter = match &note.frontmatter {
                    serde_yaml::Value::Null => None,
                    v => Some(serde_yaml::to_string(v).unwrap_or_default()),
                };
                let mtime_secs = note.mtime.and_then(|t| {
                    t.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
                });
                let size_bytes = note.size_bytes as i64;
                let tags_csv = note.tags.join(",");
                stmt.execute(params![path, title, title_lower, content, frontmatter, mtime_secs, size_bytes, tags_csv])?;
            }
        }
        // Write tags.
        {
            let mut stmt = tx.prepare("INSERT INTO tags (tag, note_path) VALUES (?1, ?2)")?;
            for note in self.notes() {
                let path = note.path.to_string_lossy().to_string();
                for tag in &note.tags {
                    stmt.execute(params![tag, path])?;
                }
            }
        }
        // Write links.
        {
            let mut stmt = tx.prepare(
                "INSERT INTO links (source_path, target, alias, kind, byte_start, byte_end)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for note in self.notes() {
                let path = note.path.to_string_lossy().to_string();
                for link in &note.links {
                    let kind = link_kind_str(link.kind);
                    stmt.execute(params![
                        path,
                        link.target.to_lowercase(),
                        link.alias,
                        kind,
                        link.byte_range.start as i64,
                        link.byte_range.end as i64,
                    ])?;
                }
            }
        }
        // Schema version.
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Load a [`NoteIndex`] from a SQLite database at `db_path`. If
    /// the file does not exist, returns an empty index. If the
    /// schema version doesn't match, returns an error.
    pub fn load_from_sqlite(db_path: &Path) -> Result<Self, IndexDbError> {
        let mut index = Self::default();
        if !db_path.is_file() {
            return Ok(index);
        }
        let conn = Connection::open(db_path)?;
        // Schema version check.
        let stored_version: i32 = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| {
                    let s: String = row.get(0)?;
                    Ok(s.parse::<i32>().unwrap_or(0))
                },
            )
            .unwrap_or(0);
        if stored_version != SCHEMA_VERSION {
            return Err(IndexDbError::SchemaVersion {
                db: stored_version,
                expected: SCHEMA_VERSION,
            });
        }

        // Load notes.
        let mut notes_map: BTreeMap<PathBuf, Note> = BTreeMap::new();
        {
            let mut stmt = conn.prepare(
                "SELECT path, title, content, frontmatter, mtime_secs, size_bytes, tags_csv
                 FROM notes",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let path: String = row.get(0)?;
                let title: String = row.get(1)?;
                let content: String = row.get(2)?;
                let frontmatter: Option<String> = row.get(3)?;
                let mtime_secs: Option<i64> = row.get(4)?;
                let size_bytes: i64 = row.get(5)?;
                let tags_csv: String = row.get(6)?;
                let frontmatter = match frontmatter {
                    Some(s) if !s.is_empty() => {
                        serde_yaml::from_str(&s).unwrap_or(serde_yaml::Value::Null)
                    }
                    _ => serde_yaml::Value::Null,
                };
                let tags: Vec<String> = if tags_csv.is_empty() {
                    Vec::new()
                } else {
                    tags_csv.split(',').map(|s| s.to_string()).collect()
                };
                let mtime = mtime_secs.and_then(|s| UNIX_EPOCH.checked_add(Duration::from_secs(s as u64)));
                let note = Note {
                    path: PathBuf::from(&path),
                    title,
                    content,
                    frontmatter,
                    links: Vec::new(), // filled in from the links table below
                    tags,
                    callouts: Vec::new(),
                    mtime,
                    size_bytes: size_bytes as u64,
                };
                notes_map.insert(PathBuf::from(&path), note);
            }
        }
        // Load links.
        {
            let mut stmt = conn.prepare(
                "SELECT source_path, target, alias, kind, byte_start, byte_end FROM links",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let source: String = row.get(0)?;
                let target: String = row.get(1)?;
                let alias: Option<String> = row.get(2)?;
                let kind_str: String = row.get(3)?;
                let start: i64 = row.get(4)?;
                let end: i64 = row.get(5)?;
                let kind = parse_link_kind(&kind_str).ok_or_else(|| {
                    IndexDbError::Malformed {
                        table: "links".to_string(),
                        message: format!("unknown kind: {kind_str}"),
                    }
                })?;
                let link = Link {
                    target,
                    alias,
                    kind,
                    byte_range: (start as usize)..(end as usize),
                };
                if let Some(note) = notes_map.get_mut(Path::new(&source)) {
                    note.links.push(link);
                }
            }
        }

        // Rebuild in-memory indexes from the loaded notes.
        for (path, note) in notes_map {
            index.insert_loaded_note(path, note);
        }
        Ok(index)
    }

    /// Internal helper: insert a Note that was loaded from SQLite
    /// into the in-memory indexes (without re-reading from disk).
    fn insert_loaded_note(&mut self, path: PathBuf, note: Note) {
        let title_lower = note.title.to_lowercase();
        self.by_title_lower.insert(title_lower.clone(), path.clone());
        for tag in &note.tags {
            self.by_tag
                .entry(tag.clone())
                .or_default()
                .insert(path.clone());
        }
        let mut targets = BTreeSet::new();
        for link in &note.links {
            let key = crate::index::link_key(&link.target);
            if key.is_empty() {
                continue;
            }
            self.backlinks.entry(key.clone()).or_default().insert(path.clone());
            targets.insert(key);
        }
        self.outgoing.insert(path.clone(), targets);
        self.notes.insert(path, note);
    }
}

fn init_schema(conn: &mut Connection) -> Result<(), IndexDbError> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS notes (
            path          TEXT PRIMARY KEY,
            title         TEXT NOT NULL,
            title_lower   TEXT NOT NULL,
            content       TEXT NOT NULL,
            frontmatter   TEXT,
            mtime_secs    INTEGER,
            size_bytes    INTEGER NOT NULL,
            tags_csv      TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX IF NOT EXISTS notes_title_lower_idx ON notes(title_lower);
        CREATE TABLE IF NOT EXISTS tags (
            tag         TEXT NOT NULL,
            note_path   TEXT NOT NULL,
            PRIMARY KEY (tag, note_path)
        );
        CREATE INDEX IF NOT EXISTS tags_tag_idx ON tags(tag);
        CREATE TABLE IF NOT EXISTS links (
            source_path  TEXT NOT NULL,
            target       TEXT NOT NULL,
            alias        TEXT,
            kind         TEXT NOT NULL,
            byte_start   INTEGER NOT NULL,
            byte_end     INTEGER NOT NULL,
            PRIMARY KEY (source_path, byte_start)
        );
        CREATE INDEX IF NOT EXISTS links_target_idx ON links(target);
        CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )?;
    Ok(())
}

fn link_kind_str(kind: LinkKind) -> &'static str {
    match kind {
        LinkKind::Wiki => "wiki",
        LinkKind::WikiHeading => "wiki-heading",
        LinkKind::WikiBlock => "wiki-block",
        LinkKind::WikiSection => "wiki-section",
        LinkKind::Embed => "embed",
        LinkKind::Markdown => "markdown",
    }
}

fn parse_link_kind(s: &str) -> Option<LinkKind> {
    match s {
        "wiki" => Some(LinkKind::Wiki),
        "wiki-heading" => Some(LinkKind::WikiHeading),
        "wiki-block" => Some(LinkKind::WikiBlock),
        "wiki-section" => Some(LinkKind::WikiSection),
        "embed" => Some(LinkKind::Embed),
        "markdown" => Some(LinkKind::Markdown),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_db(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("tungsten-db-{label}-{pid}-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn build_small_vault(dir: &Path) -> NoteIndex {
        use std::fs;
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("Alpha.md"), "# Alpha\n\nlinks to [[Beta]]\n").unwrap();
        fs::write(dir.join("Beta.md"), "# Beta\n\nalpha: see [[Alpha]]\n").unwrap();
        fs::write(dir.join("Gamma.md"), "# Gamma\n\nno links\n").unwrap();
        NoteIndex::build(dir).unwrap()
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = unique_db("roundtrip");
        let vault = dir.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let index = build_small_vault(&vault);
        let db = dir.join("index.db");
        index.save_to_sqlite(&db).unwrap();
        let loaded = NoteIndex::load_from_sqlite(&db).unwrap();
        assert_eq!(loaded.len(), 3);
        let alpha = loaded.by_title("Alpha").unwrap();
        // Targets are lowercased in storage (for index lookups).
        assert!(alpha.links.iter().any(|l| l.target == "beta"));
        let beta = loaded.by_title("Beta").unwrap();
        assert!(beta.links.iter().any(|l| l.target == "alpha"));
        // backlinks
        let bl: Vec<&str> = loaded.backlinks("Alpha").map(|n| n.title.as_str()).collect();
        assert_eq!(bl, vec!["Beta"]);
        // orphans
        let orphans: Vec<&str> = loaded.orphans().iter().map(|n| n.title.as_str()).collect();
        assert!(orphans.contains(&"Gamma"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_missing_returns_empty() {
        let dir = unique_db("missing");
        let bogus = dir.join("not-here.db");
        let index = NoteIndex::load_from_sqlite(&bogus).unwrap();
        assert!(index.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = unique_db("parents");
        let db = dir.join("nested/dir/index.db");
        let vault = dir.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(vault.join("A.md"), "x").unwrap();
        let index = NoteIndex::build(&vault).unwrap();
        index.save_to_sqlite(&db).unwrap();
        assert!(db.is_file());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_then_save_again_replaces() {
        let dir = unique_db("replace");
        let vault = dir.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(vault.join("A.md"), "x").unwrap();
        let mut index = NoteIndex::build(&vault).unwrap();
        let db = dir.join("index.db");
        index.save_to_sqlite(&db).unwrap();
        // Add a new note
        std::fs::write(vault.join("B.md"), "y").unwrap();
        index.update_note(&vault.join("B.md")).unwrap();
        index.save_to_sqlite(&db).unwrap();
        let loaded = NoteIndex::load_from_sqlite(&db).unwrap();
        assert_eq!(loaded.len(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn schema_version_mismatch_errors() {
        let dir = unique_db("version");
        let db = dir.join("index.db");
        {
            let mut conn = Connection::open(&db).unwrap();
            init_schema(&mut conn).unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
                params!["999"],
            )
            .unwrap();
        }
        let err = NoteIndex::load_from_sqlite(&db).unwrap_err();
        assert!(matches!(err, IndexDbError::SchemaVersion { .. }));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn roundtrip_preserves_tags() {
        let dir = unique_db("tags");
        let vault = dir.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(
            vault.join("A.md"),
            "---\ntags: [rust, perf]\n---\n# A\n\nbody #cli\n",
        )
        .unwrap();
        let index = NoteIndex::build(&vault).unwrap();
        let db = dir.join("index.db");
        index.save_to_sqlite(&db).unwrap();
        let loaded = NoteIndex::load_from_sqlite(&db).unwrap();
        let a = loaded.by_title("A").unwrap();
        assert!(a.tags.contains(&"rust".to_string()));
        assert!(a.tags.contains(&"perf".to_string()));
        assert!(a.tags.contains(&"cli".to_string()));
        std::fs::remove_dir_all(&dir).ok();
    }
}
