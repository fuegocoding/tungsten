//! Template registry and renderer (M1.2).
//!
//! Templates are Markdown files with `{{name}}` variable
//! substitutions. The renderer supports:
//!
//! - `{{title}}`, `{{date}}`, `{{time}}` — the standard
//!   vars
//! - `{{date:FORMAT}}` — date in a strftime-like format
//!   (e.g. `{{date:%Y-%m-%d}}`)
//! - `{{time:FORMAT}}` — same, for time
//! - `{{var}}` — any user-defined variable
//!
//! The template is the input to `NoteCreator::create` when
//! the user picks a template from the picker. A registry
//! discovers every `.md` file under a configured
//! `templates/` folder, and `render` interpolates the
//! variables.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, NaiveDate};

use crate::notes_io::{render_template as base_render, TemplateVars};

/// One template: a name, optional description, the
/// template body, and the path it was loaded from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub name: String,
    pub path: PathBuf,
    pub body: String,
    /// Optional description from the first paragraph
    /// of the template's body.
    pub description: Option<String>,
}

/// A registry of templates, keyed by name.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TemplateRegistry {
    templates: BTreeMap<String, Template>,
}

impl TemplateRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a template. The most recent registration wins
    /// on duplicate `name`.
    pub fn insert(&mut self, template: Template) {
        self.templates.insert(template.name.clone(), template);
    }

    pub fn get(&self, name: &str) -> Option<&Template> {
        self.templates.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Template)> {
        self.templates.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn len(&self) -> usize {
        self.templates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }
}

/// Discover templates under `<vault>/Templates/` and
/// `<vault>/.obsidian/templates/`. Files must end in `.md`.
pub fn discover(vault_root: &Path) -> TemplateRegistry {
    let mut r = TemplateRegistry::new();
    for dir in [
        vault_root.join("Templates"),
        vault_root.join(".obsidian").join("templates"),
    ] {
        load_from_dir(&dir, &mut r);
    }
    r
}

fn load_from_dir(dir: &Path, registry: &mut TemplateRegistry) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
            .to_string();
        let description = first_paragraph(&content);
        registry.insert(Template {
            name,
            path,
            body: content,
            description,
        });
    }
}

fn first_paragraph(content: &str) -> Option<String> {
    let mut in_block = false;
    let mut buf = String::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !in_block {
            if trimmed.starts_with("---") {
                in_block = !in_block;
                continue;
            }
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            return Some(trimmed.to_string());
        }
        if trimmed.starts_with("---") {
            in_block = !in_block;
        }
    }
    if buf.is_empty() {
        None
    } else {
        Some(buf.trim().to_string())
    }
}

/// Built-in vars always available, even when no caller
/// provided them.
pub fn default_vars(now: DateTime<Local>, title: String) -> TemplateVars {
    TemplateVars::from_datetime(now, title)
}

/// Render `template` with `vars`. Wraps the lower-level
/// `render_template` so callers can pass a single struct.
pub fn render(template: &str, vars: &TemplateVars) -> String {
    base_render(template, vars)
}

/// Build a `TemplateVars` from user-supplied key/value
/// pairs. The standard `{{date}}`, `{{time}}`, `{{title}}`
/// are always populated from `now` and `title`; the
/// remaining entries are passed through to the renderer.
pub fn vars_with_extras<I, K, V>(now: DateTime<Local>, title: String, extras: I) -> TemplateVars
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    let mut base = default_vars(now, title);
    for (k, v) in extras {
        base.extras.insert(k.into(), v.into());
    }
    base
}

/// Parse a `{{date:FORMAT}}` or `{{time:FORMAT}}` token
/// and return the formatted string. Returns `None` if the
/// token isn't a `date:` or `time:` token.
pub fn format_date_token(token: &str, now: DateTime<Local>) -> Option<String> {
    let inner = token.trim_matches(|c: char| c == '{' || c == '}');
    if let Some(fmt) = inner.strip_prefix("date:") {
        Some(now.format(fmt).to_string())
    } else if let Some(fmt) = inner.strip_prefix("time:") {
        Some(now.format(fmt).to_string())
    } else {
        None
    }
}

/// Convert a `TemplateVars`-style date (already rendered)
/// to a `NaiveDate` for the calendar subsystem.
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> std::path::PathBuf {
        let base = std::env::temp_dir();
        let unique = format!(
            "tungsten-template-test-{}-{}",
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
    fn discover_finds_md_files() {
        let dir = tempdir();
        let t = dir.join("Templates");
        fs::create_dir_all(&t).unwrap();
        fs::write(t.join("daily.md"), "# {{date}}\n").unwrap();
        fs::write(t.join("meeting.md"), "Meeting notes.\n").unwrap();
        fs::write(t.join("not-md.txt"), "x").unwrap();
        let r = discover(&dir);
        assert_eq!(r.len(), 2);
        assert!(r.get("daily").is_some());
        assert!(r.get("meeting").is_some());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn discover_supports_obsidian_path() {
        let dir = tempdir();
        let t = dir.join(".obsidian/templates");
        fs::create_dir_all(&t).unwrap();
        fs::write(t.join("project.md"), "Project plan.\n").unwrap();
        let r = discover(&dir);
        assert_eq!(r.len(), 1);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn first_paragraph_skips_heading() {
        let s = "# Heading\n\nThis is the description.\n\nMore body.\n";
        assert_eq!(first_paragraph(s).as_deref(), Some("This is the description."));
    }

    #[test]
    fn first_paragraph_skips_frontmatter() {
        let s = "---\ntitle: foo\n---\nReal content.\n";
        assert_eq!(first_paragraph(s).as_deref(), Some("Real content."));
    }

    #[test]
    fn render_substitutes_title() {
        let now = Local::now();
        let vars = default_vars(now, "My Note".into());
        let rendered = render("# {{title}}\n\nbody", &vars);
        assert!(rendered.contains("# My Note"));
    }

    #[test]
    fn format_date_token_recognizes_date_prefix() {
        let now = Local::now();
        let result = format_date_token("{{date:%Y}}", now);
        let expected = now.format("%Y").to_string();
        assert_eq!(result, Some(expected));
    }

    #[test]
    fn format_date_token_recognizes_time_prefix() {
        let now = Local::now();
        let result = format_date_token("{{time:%H}}", now);
        let expected = now.format("%H").to_string();
        assert_eq!(result, Some(expected));
    }

    #[test]
    fn format_date_token_returns_none_for_other() {
        let now = Local::now();
        assert!(format_date_token("{{title}}", now).is_none());
    }

    #[test]
    fn vars_with_extras_includes_user_values() {
        let now = Local::now();
        let vars = vars_with_extras(
            now,
            "X".into(),
            vec![("project".to_string(), "alpha".to_string())],
        );
        let rendered = render("Project: {{project}}", &vars);
        assert_eq!(rendered, "Project: alpha");
    }
}
