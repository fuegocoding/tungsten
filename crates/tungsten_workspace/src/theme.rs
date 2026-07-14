//! Theme system (M4.x).
//!
//! Obsidian themes are CSS files under `.obsidian/themes/`
//! that override a fixed set of CSS variables. The set of
//! variables is small and well-documented: `--background-primary`,
//! `--background-secondary`, `--text-normal`, `--text-muted`,
//! `--interactive-accent`, etc.
//!
//! This module:
//!
//! - Defines the [`Theme`] struct that represents a theme
//! - Parses Obsidian-style theme files (CSS) into a flat
//!   map of variable name → value
//! - Lists installed themes from `.obsidian/themes/`
//! - Exposes a [`base_theme`] that produces the default
//!   Tungsten palette, matching Obsidian's "moonstone" (dark
//!   blue-gray) closely enough that users transitioning from
//!   Obsidian won't be shocked
//!
//! Rendering is the GPUI side's job; this module only
//! describes what to render.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// A single theme: a name, an optional author, and a map of
/// CSS variable overrides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    #[serde(default)]
    pub author: String,
    /// Whether the theme is intended for light or dark
    /// backgrounds. `None` means the theme is ambiguous (e.g.
///     it relies on the system preference).
    #[serde(default)]
    pub mode: Option<ThemeMode>,
    /// CSS variable name → value. Values are raw strings;
    /// the renderer interprets them.
    pub variables: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    Light,
    Dark,
}

impl ThemeMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
}

/// The list of CSS variables Tungsten understands. Themes
/// are free to define additional variables; these are the
/// ones the UI reads.
pub const KNOWN_VARIABLES: &[&str] = &[
    "--background-primary",
    "--background-primary-alt",
    "--background-secondary",
    "--background-secondary-alt",
    "--background-modifier-border",
    "--background-modifier-hover",
    "--text-normal",
    "--text-muted",
    "--text-faint",
    "--text-accent",
    "--text-on-accent",
    "--interactive-accent",
    "--interactive-accent-hover",
    "--interactive-normal",
    "--interactive-hover",
    "--font-interface",
    "--font-text",
    "--font-monospace",
    "--font-size-small",
    "--font-size-normal",
    "--font-size-large",
    "--link-color",
    "--link-color-hover",
    "--tag-color",
    "--tag-background",
    "--callout-default",
    "--callout-note",
    "--callout-warning",
    "--callout-tip",
    "--callout-danger",
];

/// Parse an Obsidian-style CSS theme file. Variable
/// declarations look like:
///
/// ```css
/// :root {
///     --background-primary: #1e1e1e;
///     --text-normal: #dcddde;
/// }
/// ```
///
/// Or the older `.theme-dark` selector form. Both are
/// accepted. Multiple declarations on one line are also
/// accepted.
pub fn parse_css(content: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("/*") {
            continue;
        }
        // Strip the trailing `}` so the last declaration's
        // value isn't polluted.
        let body = trimmed.trim_end_matches('}');
        // Walk every `--name: value` occurrence. This is
        // more robust than splitting on `;` because some
        // inline declarations share a line with a CSS
        // selector.
        let bytes = body.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
                // Read the name.
                let start = i;
                let mut j = i + 2;
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric()
                        || bytes[j] == b'-'
                        || bytes[j] == b'_')
                {
                    j += 1;
                }
                let name = body[start..j].to_string();
                // Skip whitespace, expect `:`, skip more
                // whitespace, read the value up to `;`, `}`,
                // or end.
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b':' {
                    j += 1;
                    while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                        j += 1;
                    }
                    let val_start = j;
                    while j < bytes.len()
                        && bytes[j] != b';'
                        && bytes[j] != b'}'
                        && bytes[j] != b'\n'
                    {
                        j += 1;
                    }
                    let value = body[val_start..j].trim().to_string();
                    if !value.is_empty() {
                        out.insert(name, value);
                    }
                    i = j;
                    continue;
                }
            }
            i += 1;
        }
    }
    out
}

/// Read the theme's metadata from the head of a CSS file.
/// Obsidian themes begin with a comment block like:
///
/// ```css
/// /* @name        Moonstone
///    @author      Your Name
///    @version     1.0.0
///    @description Default dark theme
///    @dark        */
/// ```
pub fn parse_metadata(content: &str) -> ThemeMeta {
    let mut meta = ThemeMeta::default();
    let mut in_block = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if !in_block && trimmed.starts_with("/*") {
            in_block = true;
        }
        if in_block {
            if let Some(name) = extract_at_field(trimmed, "name") {
                meta.name = name;
            }
            if let Some(author) = extract_at_field(trimmed, "author") {
                meta.author = author;
            }
            if let Some(v) = extract_at_field(trimmed, "version") {
                meta.version = v;
            }
            if let Some(d) = extract_at_field(trimmed, "description") {
                meta.description = d;
            }
            if trimmed.contains("@dark") {
                meta.mode = Some(ThemeMode::Dark);
            } else if trimmed.contains("@light") {
                meta.mode = Some(ThemeMode::Light);
            }
            if trimmed.ends_with("*/") {
                break;
            }
        }
    }
    meta
}

fn extract_at_field(line: &str, field: &str) -> Option<String> {
    let marker = format!("@{field}");
    let lower = line.to_ascii_lowercase();
    if let Some(idx) = lower.find(&marker) {
        let after = line[idx + marker.len()..].trim_start_matches(':').trim();
        // The field ends at `*/`, end of line, or the next
        // `*/`.
        let end = after.find("*/").unwrap_or(after.len());
        return Some(after[..end].trim().to_string());
    }
    None
}

/// Metadata extracted from the theme file's head comment.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ThemeMeta {
    pub name: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub mode: Option<ThemeMode>,
}

/// Build a [`Theme`] from a file on disk.
pub fn load_from_path(path: &Path) -> std::io::Result<Theme> {
    let content = std::fs::read_to_string(path)?;
    let meta = parse_metadata(&content);
    let variables = parse_css(&content);
    let fallback_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();
    Ok(Theme {
        name: if meta.name.is_empty() { fallback_name } else { meta.name },
        author: meta.author,
        mode: meta.mode,
        variables,
    })
}

/// The Tungsten base theme: a dark blue-gray palette that
/// matches Obsidian's "moonstone" defaults.
pub fn base_theme() -> Theme {
    let mut variables = BTreeMap::new();
    for (k, v) in [
        ("--background-primary", "#1f2024"),
        ("--background-primary-alt", "#1a1b1e"),
        ("--background-secondary", "#28292d"),
        ("--background-secondary-alt", "#2f3034"),
        ("--background-modifier-border", "#3a3b40"),
        ("--background-modifier-hover", "#3f4045"),
        ("--text-normal", "#dcddde"),
        ("--text-muted", "#999a9b"),
        ("--text-faint", "#656667"),
        ("--text-accent", "#7f6df2"),
        ("--text-on-accent", "#ffffff"),
        ("--interactive-accent", "#7f6df2"),
        ("--interactive-accent-hover", "#9388f5"),
        ("--interactive-normal", "#cdcdcd"),
        ("--interactive-hover", "#dcddde"),
        ("--font-interface", "system-ui, -apple-system, sans-serif"),
        ("--font-text", "system-ui, -apple-system, sans-serif"),
        ("--font-monospace", "ui-monospace, monospace"),
        ("--font-size-small", "0.85em"),
        ("--font-size-normal", "1em"),
        ("--font-size-large", "1.2em"),
        ("--link-color", "#7f6df2"),
        ("--link-color-hover", "#9388f5"),
        ("--tag-color", "#dcddde"),
        ("--tag-background", "#3f4045"),
        ("--callout-default", "#3a3b40"),
        ("--callout-note", "#5b8def"),
        ("--callout-warning", "#e0a32e"),
        ("--callout-tip", "#3fb950"),
        ("--callout-danger", "#e5534b"),
    ] {
        variables.insert(k.to_string(), v.to_string());
    }
    Theme {
        name: "Tungsten Default".into(),
        author: "Tungsten".into(),
        mode: Some(ThemeMode::Dark),
        variables,
    }
}

/// Discover all installed themes under
/// `.obsidian/themes/`. Files without a `.css` extension
/// are skipped.
pub fn list_themes(obsidian_dir: &Path) -> std::io::Result<Vec<Theme>> {
    let themes_dir = obsidian_dir.join("themes");
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&themes_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("css") {
            continue;
        }
        if let Ok(t) = load_from_path(&path) {
            out.push(t);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_css_extracts_root_variables() {
        let s = ":root {\n  --background-primary: #1e1e1e;\n  --text-normal: #dcddde;\n}\n";
        let m = parse_css(s);
        assert_eq!(m.get("--background-primary").unwrap(), "#1e1e1e");
        assert_eq!(m.get("--text-normal").unwrap(), "#dcddde");
    }

    #[test]
    fn parse_css_handles_inline_declarations() {
        let s = ".theme-dark { --background-primary: #1e1e1e; --text-normal: #fff; }";
        let m = parse_css(s);
        eprintln!("DBG: m={:?}", m);
        assert_eq!(m.get("--background-primary").unwrap(), "#1e1e1e");
        assert_eq!(m.get("--text-normal").unwrap(), "#fff");
    }

    #[test]
    fn parse_css_ignores_comments() {
        let s = "/* a comment */\n:root {\n--bg: #000;\n}\n";
        let m = parse_css(s);
        assert_eq!(m.get("--bg").unwrap(), "#000");
    }

    #[test]
    fn parse_metadata_extracts_at_fields() {
        let s = "/* @name        MyTheme\n   @author      Alice\n   @version     1.2.3\n   @description Cool theme\n   @dark        */\n";
        let m = parse_metadata(s);
        assert_eq!(m.name, "MyTheme");
        assert_eq!(m.author, "Alice");
        assert_eq!(m.version, "1.2.3");
        assert_eq!(m.description, "Cool theme");
        assert_eq!(m.mode, Some(ThemeMode::Dark));
    }

    #[test]
    fn theme_mode_roundtrip() {
        assert_eq!(ThemeMode::from_str("light"), Some(ThemeMode::Light));
        assert_eq!(ThemeMode::from_str("DARK"), Some(ThemeMode::Dark));
        assert_eq!(ThemeMode::from_str("nope"), None);
        assert_eq!(ThemeMode::Light.as_str(), "light");
    }

    #[test]
    fn base_theme_has_all_known_variables() {
        let t = base_theme();
        for var in KNOWN_VARIABLES {
            assert!(t.variables.contains_key(*var), "missing: {}", var);
        }
        assert_eq!(t.mode, Some(ThemeMode::Dark));
    }

    #[test]
    fn base_theme_distinct_callout_colors() {
        let t = base_theme();
        let note = t.variables.get("--callout-note").unwrap();
        let warning = t.variables.get("--callout-warning").unwrap();
        assert_ne!(note, warning);
    }

    #[test]
    fn list_themes_finds_css_files() {
        use std::fs;
        let dir = std::env::temp_dir().join(format!(
            "tungsten-theme-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let obs = dir.join(".obsidian/themes");
        fs::create_dir_all(&obs).unwrap();
        fs::write(
            obs.join("moonstone.css"),
            "/* @name Moonstone */ :root { --bg: #1e1e1e; }",
        )
        .unwrap();
        fs::write(obs.join("not-css.txt"), "x").unwrap();
        let themes = list_themes(&obs.parent().unwrap()).unwrap();
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].name, "Moonstone");
        fs::remove_dir_all(&dir).ok();
    }
}
