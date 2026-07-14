//! Note creation, daily notes, unique notes, and template rendering.
//!
//! This is the "writing" side of the workspace: given a vault, let
//! the user (or a plugin) create new notes at predictable paths,
//! with templates applied.
//!
//! Three creation modes:
//! 1. **Explicit path** — `create(rel_path, content)` writes a note
//!    at the given path inside the vault.
//! 2. **Daily note** — `create_daily(now)` writes
//!    `<vault>/<daily_folder>/<date>.md`, using the configured
//!    template if one is set. The same call is idempotent: if
//!    today's daily note already exists, the path is returned
//!    without modification.
//! 3. **Unique note** — `create_unique(prefix, now)` writes a
//!    Zettelkasten-timestamped note in the vault root (or a
//!    configured folder). The pattern is `<prefix>YYYYMMDDHHMMSS.md`
//!    when a prefix is given, or just `YYYYMMDDHHMMSS.md` otherwise.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeZone};

use crate::note::Note;
use crate::Vault;

/// Errors that can occur while creating a note.
#[derive(Debug, thiserror::Error)]
pub enum NoteCreateError {
    #[error("refusing to write outside the vault: {path}")]
    OutsideVault { path: PathBuf },
    #[error("parent directory does not exist: {0}")]
    MissingParent(PathBuf),
    #[error("file already exists: {0}")]
    AlreadyExists(PathBuf),
    #[error("template not found: {0}")]
    TemplateNotFound(PathBuf),
    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Variables available to templates.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TemplateVars {
    pub title: String,
    pub date: NaiveDate,
    pub time: NaiveTime,
    /// User-supplied extras: project name, author, anything
    /// a template wants. `{{key}}` lookups fall back here
    /// when the standard vars don't match.
    pub extras: std::collections::BTreeMap<String, String>,
}

impl TemplateVars {
    /// Build a `TemplateVars` from a `DateTime<Local>` and an
    /// optional title. The title is what the note will be called
    /// (usually the filename without extension).
    pub fn from_datetime(now: DateTime<Local>, title: String) -> Self {
        Self {
            title,
            date: now.date_naive(),
            time: now.time(),
            extras: std::collections::BTreeMap::new(),
        }
    }
}

/// A note creator bound to a vault.
pub struct NoteCreator<'a> {
    vault: &'a Vault,
    daily_folder: PathBuf,
    daily_filename_format: String,
    template_dir: Option<PathBuf>,
    daily_template: Option<String>,
}

impl<'a> NoteCreator<'a> {
    pub fn new(vault: &'a Vault) -> Self {
        Self {
            vault,
            daily_folder: PathBuf::from("Journal"),
            daily_filename_format: "%Y-%m-%d".to_string(),
            template_dir: None,
            daily_template: None,
        }
    }

    /// Override the folder where daily notes are written.
    /// Relative paths resolve against the vault root.
    pub fn with_daily_folder(mut self, folder: impl Into<PathBuf>) -> Self {
        self.daily_folder = folder.into();
        self
    }

    /// Override the daily-note filename format (strftime syntax).
    pub fn with_daily_filename_format(mut self, fmt: impl Into<String>) -> Self {
        self.daily_filename_format = fmt.into();
        self
    }

    /// Set the templates directory. When set, daily-note creation
    /// will look for a file named `daily.md` inside this directory
    /// and use it as the template.
    pub fn with_template_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.template_dir = Some(dir.into());
        self
    }

    /// Use this literal string as the daily-note template (instead
    /// of looking it up in `template_dir`).
    pub fn with_daily_template(mut self, tpl: impl Into<String>) -> Self {
        self.daily_template = Some(tpl.into());
        self
    }

    /// Create a new note at `relative_path` (resolved against the
    /// vault root) with the given content. Parent directories are
    /// created. Errors if the file already exists.
    pub fn create(
        &self,
        relative_path: &Path,
        content: &str,
    ) -> Result<PathBuf, NoteCreateError> {
        let abs = self.resolve(relative_path)?;
        if abs.exists() {
            return Err(NoteCreateError::AlreadyExists(abs));
        }
        if let Some(parent) = abs.parent() {
            if !parent.is_dir() {
                fs::create_dir_all(parent).map_err(|source| NoteCreateError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
        }
        fs::write(&abs, content).map_err(|source| NoteCreateError::Io {
            path: abs.clone(),
            source,
        })?;
        Ok(abs)
    }

    /// Create a note at an *absolute* path. Unlike
    /// [`Self::create`], the path is not resolved against the
    /// vault root — it's used as-is. The note is parsed and
    /// returned so the caller can use it directly.
    pub fn create_with_path(
        &self,
        abs_path: &Path,
        _title: &str,
        body: &str,
    ) -> Result<Note, NoteCreateError> {
        if abs_path.exists() {
            return Err(NoteCreateError::AlreadyExists(abs_path.to_path_buf()));
        }
        if let Some(parent) = abs_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|source| NoteCreateError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
        }
        fs::write(abs_path, body).map_err(|source| NoteCreateError::Io {
            path: abs_path.to_path_buf(),
            source,
        })?;
        crate::note::Note::read(abs_path).map_err(|source| NoteCreateError::Io {
            path: abs_path.to_path_buf(),
            source,
        })
    }

    /// Read a note from an absolute path. The note is parsed
    /// via the standard [`Note::read`] loader.
    pub fn read(&self, abs_path: &Path) -> Result<Note, NoteCreateError> {
        Note::read(abs_path).map_err(|source| NoteCreateError::Io {
            path: abs_path.to_path_buf(),
            source,
        })
    }

    /// Create (or locate) today's daily note. Idempotent: if the
    /// file already exists, the path is returned without modifying
    /// the file.
    pub fn create_daily(
        &self,
        now: DateTime<Local>,
    ) -> Result<PathBuf, NoteCreateError> {
        let filename = now.format(&self.daily_filename_format).to_string() + ".md";
        let rel = self.daily_folder.join(filename);
        let abs = self.resolve(&rel)?;
        if abs.exists() {
            return Ok(abs);
        }
        let title = now.format(&self.daily_filename_format).to_string();
        let vars = TemplateVars::from_datetime(now, title);
        let body = match (&self.daily_template, &self.template_dir) {
            (Some(tpl), _) => render_template(tpl, &vars),
            (None, Some(dir)) => {
                let tpl_path = dir.join("daily.md");
                if tpl_path.is_file() {
                    let raw = fs::read_to_string(&tpl_path).map_err(|source| {
                        NoteCreateError::Io {
                            path: tpl_path.clone(),
                            source,
                        }
                    })?;
                    render_template(&raw, &vars)
                } else {
                    default_daily_note(&vars)
                }
            }
            (None, None) => default_daily_note(&vars),
        };
        self.create(&rel, &body)
    }

    /// Create a Zettelkasten-timestamped unique note. The pattern
    /// is `<prefix>YYYYMMDDHHMMSS.md` when a prefix is given, or
    /// just `YYYYMMDDHHMMSS.md` otherwise. Collisions on the same
    /// second are resolved by appending a counter.
    pub fn create_unique(
        &self,
        prefix: Option<&str>,
        now: DateTime<Local>,
    ) -> Result<PathBuf, NoteCreateError> {
        let stem = now.format("%Y%m%d%H%M%S").to_string();
        let base = match prefix {
            Some(p) if !p.is_empty() => format!("{p} {stem}"),
            _ => stem,
        };
        let mut filename = format!("{base}.md");
        let mut rel = PathBuf::from(&filename);
        let mut counter = 1u32;
        while self.resolve(&rel).map(|p| p.exists()).unwrap_or(false) {
            filename = format!("{base} {counter}.md");
            rel = PathBuf::from(&filename);
            counter += 1;
            if counter > 1000 {
                return Err(NoteCreateError::AlreadyExists(rel));
            }
        }
        let title = base.clone();
        let vars = TemplateVars::from_datetime(now, title);
        let body = default_unique_note(&vars);
        self.create(&rel, &body)
    }

    /// Resolve a relative path against the vault root, ensuring the
    /// result is inside the vault (defense against `..` traversal).
    fn resolve(&self, relative: &Path) -> Result<PathBuf, NoteCreateError> {
        // Reject any path that contains a `..` component. This is
        // stricter than canonicalize-based checks (which can't see
        // through non-existent files anyway) but matches the
        // principle of least surprise: a relative path with `..`
        // was probably a bug or a prompt-injection attempt.
        for component in relative.components() {
            if let std::path::Component::ParentDir = component {
                return Err(NoteCreateError::OutsideVault {
                    path: relative.to_path_buf(),
                });
            }
        }
        Ok(self.vault.root().join(relative))
    }
}

/// Render a template by substituting `{{var}}` placeholders.
///
/// Supported placeholders:
/// - `{{title}}` — note title
/// - `{{date}}` — ISO 8601 date (`YYYY-MM-DD`)
/// - `{{date:FORMAT}}` — date with strftime `FORMAT`
/// - `{{time}}` — `HH:MM` (24h)
/// - `{{time:FORMAT}}` — time with strftime `FORMAT`
///
/// Unknown placeholders are left as-is. A literal `{{` is produced
/// from `\{\{` and `}}` from `\}\}` (we don't process backslash
/// escapes; this is plain substitution).
pub fn render_template(template: &str, vars: &TemplateVars) -> String {
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            // Find the matching `}}`
            if let Some(end) = find_close(&template[i + 2..]) {
                let inner = &template[i + 2..i + 2 + end];
                out.push_str(&substitute(inner, vars));
                i += 2 + end + 2;
                continue;
            }
        }
        out.push(template[i..].chars().next().unwrap());
        i += template[i..].chars().next().unwrap().len_utf8();
    }
    out
}

fn find_close(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'}' && bytes[i + 1] == b'}' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn substitute(inner: &str, vars: &TemplateVars) -> String {
    let trimmed = inner.trim();
    if let Some(fmt) = trimmed.strip_prefix("date:") {
        return vars.date.format(fmt).to_string();
    }
    if let Some(fmt) = trimmed.strip_prefix("time:") {
        return vars.time.format(fmt).to_string();
    }
    match trimmed {
        "title" => return vars.title.clone(),
        "date" => return vars.date.format("%Y-%m-%d").to_string(),
        "time" => return vars.time.format("%H:%M").to_string(),
        _ => {}
    }
    // Fall back to user-supplied extras, then leave the
    // token verbatim (a common convention in template
    // languages so missing values are visible in the
    // output).
    if let Some(value) = vars.extras.get(trimmed) {
        return value.clone();
    }
    format!("{{{{{trimmed}}}}}")
}

fn default_daily_note(vars: &TemplateVars) -> String {
    format!(
        "# {date}\n\n## How I feel\n\n## Free write\n\n",
        date = vars.date.format("%Y-%m-%d")
    )
}

fn default_unique_note(vars: &TemplateVars) -> String {
    format!(
        "# {title}\n\ncreated {date} at {time}\n",
        title = vars.title,
        date = vars.date.format("%Y-%m-%d"),
        time = vars.time.format("%H:%M"),
    )
}

/// Read a note from disk if it exists, else `None`. Convenience used
/// by callers that want to inspect a note after creating it.
pub fn read_note_if_exists(path: &Path) -> std::io::Result<Option<Note>> {
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(Note::read(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_vault(label: &str) -> Vault {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("tungsten-create-{label}-{pid}-{n}"));
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir(dir.join(".obsidian")).unwrap();
        Vault::open(&dir).expect("vault")
    }

    #[test]
    fn create_writes_file() {
        let vault = unique_vault("create");
        let creator = NoteCreator::new(&vault);
        let path = creator.create(Path::new("note.md"), "# hello\n").unwrap();
        assert!(path.is_file());
        let body = fs::read_to_string(&path).unwrap();
        assert_eq!(body, "# hello\n");
        fs::remove_dir_all(vault.root()).ok();
    }

    #[test]
    fn create_nested_creates_parent_dirs() {
        let vault = unique_vault("nested");
        let creator = NoteCreator::new(&vault);
        let path = creator
            .create(Path::new("a/b/c/note.md"), "x")
            .unwrap();
        assert!(path.is_file());
        fs::remove_dir_all(vault.root()).ok();
    }

    #[test]
    fn create_fails_when_file_exists() {
        let vault = unique_vault("exists");
        let creator = NoteCreator::new(&vault);
        creator.create(Path::new("note.md"), "first").unwrap();
        let err = creator.create(Path::new("note.md"), "second").unwrap_err();
        assert!(matches!(err, NoteCreateError::AlreadyExists(_)));
        fs::remove_dir_all(vault.root()).ok();
    }

    #[test]
    fn create_rejects_path_traversal() {
        let vault = unique_vault("traversal");
        let creator = NoteCreator::new(&vault);
        let err = creator.create(Path::new("../escape.md"), "x").unwrap_err();
        assert!(matches!(err, NoteCreateError::OutsideVault { .. }));
        fs::remove_dir_all(vault.root()).ok();
    }

    #[test]
    fn daily_note_creates_in_journal_folder() {
        let vault = unique_vault("daily");
        let creator = NoteCreator::new(&vault);
        let now = DateTime::parse_from_rfc3339("2026-07-12T08:30:00+00:00")
            .unwrap()
            .with_timezone(&Local);
        let path = creator.create_daily(now).unwrap();
        assert!(path.is_file());
        assert!(path.ends_with("Journal/2026-07-12.md"));
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("2026-07-12"));
        fs::remove_dir_all(vault.root()).ok();
    }

    #[test]
    fn daily_note_is_idempotent() {
        let vault = unique_vault("daily-idem");
        let creator = NoteCreator::new(&vault);
        let now = DateTime::parse_from_rfc3339("2026-07-12T08:30:00+00:00")
            .unwrap()
            .with_timezone(&Local);
        let p1 = creator.create_daily(now).unwrap();
        let body1 = fs::read_to_string(&p1).unwrap();
        // Second call should not overwrite.
        let p2 = creator.create_daily(now).unwrap();
        assert_eq!(p1, p2);
        let body2 = fs::read_to_string(&p2).unwrap();
        assert_eq!(body1, body2, "idempotent: same content on second call");
        fs::remove_dir_all(vault.root()).ok();
    }

    #[test]
    fn daily_note_uses_template() {
        let vault = unique_vault("daily-tpl");
        let creator = NoteCreator::new(&vault).with_daily_template(
            "# {{date}} ({{title}})\n\nMood: __\n",
        );
        let now = DateTime::parse_from_rfc3339("2026-07-12T08:30:00+00:00")
            .unwrap()
            .with_timezone(&Local);
        let path = creator.create_daily(now).unwrap();
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("2026-07-12"));
        assert!(body.contains("Mood:"));
        fs::remove_dir_all(vault.root()).ok();
    }

    #[test]
    fn unique_note_uses_zettelkasten_timestamp() {
        let vault = unique_vault("unique");
        let creator = NoteCreator::new(&vault);
        let now = DateTime::parse_from_rfc3339("2026-07-12T08:30:45+00:00")
            .unwrap()
            .with_timezone(&Local);
        let path = creator.create_unique(Some("Note"), now).unwrap();
        let stem = path.file_stem().unwrap().to_str().unwrap();
        assert!(stem.starts_with("Note 20260712"), "stem: {stem}");
        fs::remove_dir_all(vault.root()).ok();
    }

    #[test]
    fn unique_note_collision_appends_counter() {
        let vault = unique_vault("unique-collide");
        let creator = NoteCreator::new(&vault);
        let now = Local
            .with_ymd_and_hms(2026, 7, 12, 8, 30, 45)
            .unwrap();
        let p1 = creator.create_unique(None, now).unwrap();
        let p2 = creator.create_unique(None, now).unwrap();
        assert_ne!(p1, p2);
        let n1 = p1.file_stem().unwrap().to_str().unwrap();
        let n2 = p2.file_stem().unwrap().to_str().unwrap();
        assert!(n1.contains("20260712083045"), "first stem: {n1}");
        assert!(n2.contains("20260712083045"), "second stem: {n2}");
        assert!(n2.ends_with("1"), "second collision should have counter 1, got {n2}");
        fs::remove_dir_all(vault.root()).ok();
    }

    #[test]
    fn render_template_substitutes_vars() {
        let vars = TemplateVars {
            title: "My Note".into(),
            date: NaiveDate::from_ymd_opt(2026, 7, 12).unwrap(),
            time: NaiveTime::from_hms_opt(8, 30, 0).unwrap(),
            extras: std::collections::BTreeMap::new(),
        };
        let out = render_template(
            "# {{title}}\n\n{{date}} at {{time}}\n",
            &vars,
        );
        assert_eq!(out, "# My Note\n\n2026-07-12 at 08:30\n");
    }

    #[test]
    fn render_template_supports_date_format() {
        let vars = TemplateVars {
            title: "x".into(),
            date: NaiveDate::from_ymd_opt(2026, 7, 12).unwrap(),
            time: NaiveTime::from_hms_opt(8, 30, 0).unwrap(),
            extras: std::collections::BTreeMap::new(),
        };
        let out = render_template("{{date:%Y/%m/%d}}\n{{time:%H:%M:%S}}", &vars);
        assert_eq!(out, "2026/07/12\n08:30:00");
    }

    #[test]
    fn render_template_leaves_unknown_placeholder() {
        let vars = TemplateVars {
            title: "x".into(),
            date: NaiveDate::from_ymd_opt(2026, 7, 12).unwrap(),
            time: NaiveTime::from_hms_opt(8, 30, 0).unwrap(),
            extras: std::collections::BTreeMap::new(),
        };
        let out = render_template("hello {{foo}}", &vars);
        assert_eq!(out, "hello {{foo}}");
    }
}
