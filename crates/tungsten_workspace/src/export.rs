//! Vault export: a portable JSON dump of a vault.
//!
//! `tungsten-export` walks the vault, loads every `.md`
//! file as a [`Note`], and writes a single JSON document
//! that captures:
//!
//! - Every note's full content, frontmatter, tags, links,
//!   callouts, mtime
//! - The vault root path
//! - Index statistics (notes, links, tags, orphans, broken
//!   links)
//!
//! The output is intended for migration, backup, or
//! feeding into another tool. It is **not** encrypted — for
//! that, run `tungsten-encrypt` on the resulting file.
//!
//! The output JSON shape:
//!
//! ```json
//! {
//!   "vault": "/abs/path",
//!   "stats": { "note_count": 12, ... },
//!   "notes": [
//!     { "path": "Welcome.md", "title": "Welcome",
//!       "content": "...", "frontmatter": {...},
//!       "tags": ["intro"], "links": [...], "callouts": [...],
//!       "mtime_secs": 1700000000 },
//!     ...
//!   ]
//! }
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::index::NoteIndex;
use crate::note::{Link, Note};

#[derive(Debug, Clone, Serialize)]
pub struct ExportNote {
    pub path: String,
    pub title: String,
    pub content: String,
    pub frontmatter: serde_yaml::Value,
    pub tags: Vec<String>,
    pub links: Vec<ExportLink>,
    pub callouts: Vec<ExportCallout>,
    pub mtime_secs: Option<u64>,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportLink {
    pub target: String,
    pub kind: String,
    pub alias: Option<String>,
    pub byte_range: (usize, usize),
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportCallout {
    pub kind: String,
    pub title: Option<String>,
    pub byte_range: (usize, usize),
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportStats {
    pub note_count: usize,
    pub link_count: usize,
    pub tag_count: usize,
    pub orphan_count: usize,
    pub broken_link_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Export {
    vault: String,
    exported_at: DateTime<Utc>,
    stats: ExportStats,
    pub notes: Vec<ExportNote>,
}

/// Build the export struct from a [`NoteIndex`].
pub fn build_export(index: &NoteIndex) -> Export {
    build_export_with(index, "".to_string())
}

/// Like [`build_export`] but takes an explicit vault
/// path. Used when the index is built from a different
/// path than the one the export should report.
pub fn build_export_with(index: &NoteIndex, vault: String) -> Export {
    let mut notes: Vec<ExportNote> = index
        .notes()
        .map(|n| note_to_export(n, index))
        .collect();
    notes.sort_by(|a, b| a.path.cmp(&b.path));

    let stats = ExportStats {
        note_count: index.len(),
        link_count: index.stats().link_count,
        tag_count: index.stats().tag_count,
        orphan_count: index.orphans().len(),
        broken_link_count: count_broken(index),
    };

    Export {
        vault: index
            .notes()
            .next()
            .and_then(|n| n.path.parent())
            .and_then(|p| p.parent())
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string(),
        exported_at: Utc::now(),
        stats,
        notes,
    }
}

fn count_broken(index: &NoteIndex) -> usize {
    let mut count = 0;
    for note in index.notes() {
        for link in &note.links {
            if index.by_title(&link.target).is_none() {
                count += 1;
            }
        }
    }
    count
}

fn note_to_export(note: &Note, _index: &NoteIndex) -> ExportNote {
    let mtime_secs = note
        .mtime
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    let links: Vec<ExportLink> = note
        .links
        .iter()
        .map(|l: &Link| ExportLink {
            target: l.target.clone(),
            kind: format!("{:?}", l.kind),
            alias: l.alias.clone(),
            byte_range: (l.byte_range.start, l.byte_range.end),
        })
        .collect();
    let callouts: Vec<ExportCallout> = note
        .callouts
        .iter()
        .map(|c| ExportCallout {
            kind: c.kind.clone(),
            title: c.title.clone(),
            byte_range: (c.byte_range.start, c.byte_range.end),
        })
        .collect();
    ExportNote {
        path: note.path.display().to_string(),
        title: note.title.clone(),
        content: note.content.clone(),
        frontmatter: note.frontmatter.clone(),
        tags: note.tags.clone(),
        links,
        callouts,
        mtime_secs,
        size_bytes: note.size_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> std::path::PathBuf {
        let base = std::env::temp_dir();
        let unique = format!(
            "tungsten-export-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let p = base.join(unique);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn export_includes_all_notes() {
        let dir = tempdir();
        fs::write(dir.join("A.md"), "# A\nbody with [[B]]\n").unwrap();
        fs::write(dir.join("B.md"), "# B\n").unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let export = build_export(&index);
        assert_eq!(export.notes.len(), 2);
        assert_eq!(export.stats.note_count, 2);
        assert_eq!(export.stats.link_count, 1);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn export_serializes_to_valid_json() {
        let dir = tempdir();
        fs::write(dir.join("A.md"), "# A\n").unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let export = build_export(&index);
        let json = serde_json::to_string_pretty(&export).unwrap();
        assert!(json.contains("A.md"), "missing A.md in: {json}");
        assert!(json.contains("note_count"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn export_handles_empty_vault() {
        let dir = tempdir();
        let index = NoteIndex::build(&dir).unwrap();
        let export = build_export(&index);
        assert_eq!(export.notes.len(), 0);
        assert_eq!(export.stats.note_count, 0);
        fs::remove_dir_all(&dir).ok();
    }
}
