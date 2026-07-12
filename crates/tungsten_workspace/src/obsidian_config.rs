//! Loader for the contents of a vault's `.obsidian/` config directory.
//!
//! Tungsten reads Obsidian's per-vault config format so that opening
//! an existing Obsidian vault requires no conversion. The loader
//! handles the JSON files Obsidian writes (`app.json`,
//! `appearance.json`, `core-plugins.json`, `community-plugins.json`,
//! `types.json`) and the directory-based resources (`themes/`,
//! `plugins/`, `snippets/`).
//!
//! **What is *not* in this loader:** hotkey bindings (planned for a
//! later milestone — they require a translation layer from
//! Obsidian's `modifiers` strings to GPUI's keymap), workspace layout
//! state, and the actual plugin code (handled by the Obsidian
//! compat subsystem, see PRD §4.6). What's in here is the catalog:
//! what themes are available, what plugins are installed, what
//! snippets are present, and the user-facing appearance settings.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{VaultInventory, OBSIDIAN_CONFIG_DIR};

/// All errors that can occur while loading a `.obsidian/` config.
///
/// The loader is forgiving — missing optional files are not errors —
/// but a malformed required file is. The two "required" files in
/// practice are `app.json` and `appearance.json`, both written by
/// Obsidian on every save.
#[derive(Debug, thiserror::Error)]
pub enum ObsidianConfigError {
    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("config directory does not exist: {0}")]
    MissingConfigDir(PathBuf),
}

/// The full contents of a vault's `.obsidian/` config directory.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ObsidianConfig {
    pub app: AppConfig,
    pub appearance: AppearanceConfig,
    pub inventory: VaultInventory,
    pub types: BTreeMap<String, String>,
}

/// Settings from `.obsidian/app.json`.
///
/// Obsidian writes a superset of these keys depending on version and
/// enabled features. We capture the common ones; anything we don't
/// recognize is preserved on save and ignored.
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppConfig {
    pub always_update_links: bool,
    pub new_link_format: NewLinkFormat,
    pub use_markdown_links: bool,
    pub vim_mode: bool,
    pub live_preview: bool,
    pub show_line_number: bool,
    pub spellcheck: bool,
    pub readable_line_length: bool,
    pub strict_line_breaks: bool,
    pub auto_pair_brackets: bool,
    pub auto_pair_markdown: bool,
    pub auto_pair_angle_brackets: bool,
    pub fold_headings: bool,
    pub fold_indent: bool,
    pub fold_quote: bool,
    pub fold_code: bool,
    pub fold_list: bool,
    pub fold_yaml: bool,
    pub show_inline_title: bool,
    pub show_view_header: bool,
    pub show_backlink_in_doc: bool,
    pub show_related_files: bool,
    pub show_file_filter: bool,
    pub show_emoji_shortcode: bool,
    pub show_word_count: bool,
    pub show_char_count: bool,
    pub show_reading_time: bool,
    pub use_pinyin: bool,
    pub attachments_folder: String,
    pub plugin_tracking: bool,
    pub prompt_delete: bool,
    pub confirm_creating_new_untitled: bool,
    pub always_select_new_file: bool,
    pub disabled_plugins: Vec<String>,
    pub enabled_plugins: BTreeMap<String, bool>,
}

/// Obsidian's link format enum.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NewLinkFormat {
    #[default]
    Shortest,
    ShortestPossible,
    Relative,
    Absolute,
}

/// Settings from `.obsidian/appearance.json`.
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppearanceConfig {
    pub css_theme: Option<String>,
    pub base_font_size: Option<u32>,
    pub base_font_family: Option<String>,
    pub monospace_font_family: Option<String>,
    pub accent_color: Option<String>,
    pub theme_mode: Option<String>,
    pub translucency: bool,
    pub enabled_css_snippets: Vec<String>,
}

/// A theme directory under `.obsidian/themes/<name>/`.
///
/// Each theme is a directory with a `manifest.json` and a `theme.css`
/// (or `obsidian.css` for older themes). The manifest carries the
/// display name, version, and author info used in the theme picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeInfo {
    pub name: String,
    pub path: PathBuf,
    pub display_name: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
}

/// A community plugin under `.obsidian/plugins/<id>/`.
///
/// Each plugin is a directory with a `manifest.json` and a `main.js`.
/// The manifest carries the display name, version, and description;
/// the actual JS code is loaded by the Obsidian compat subsystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInfo {
    pub id: String,
    pub path: PathBuf,
    pub display_name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub min_app_version: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
struct ThemeManifest {
    name: Option<String>,
    version: Option<String>,
    author: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PluginManifest {
    id: Option<String>,
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    #[serde(rename = "minAppVersion")]
    min_app_version: Option<String>,
}

impl ObsidianConfig {
    /// Load a vault's `.obsidian/` config from `config_dir` (which must
    /// already be the `.obsidian/` path itself, not the vault root).
    pub fn load(config_dir: &Path) -> Result<Self, ObsidianConfigError> {
        if !config_dir.is_dir() {
            return Err(ObsidianConfigError::MissingConfigDir(config_dir.to_path_buf()));
        }

        let mut config = Self::default();

        // Required-ish JSON files.
        let app_path = config_dir.join("app.json");
        if app_path.is_file() {
            config.app = read_json(&app_path)?;
        }

        let appearance_path = config_dir.join("appearance.json");
        if appearance_path.is_file() {
            config.appearance = read_json(&appearance_path)?;
        }

        // Optional JSON files.
        if let Ok(core) = read_json::<BTreeMap<String, bool>>(&config_dir.join("core-plugins.json")) {
            config.inventory.core_plugins = core;
        }
        if let Ok(ids) = read_json::<Vec<String>>(&config_dir.join("community-plugins.json")) {
            config.inventory.community_plugin_ids = ids;
        }
        if let Ok(types) = read_json::<TypesFile>(&config_dir.join("types.json")) {
            config.types = types.types;
        }

        // Directory-based resources.
        config.inventory.themes = discover_themes(&config_dir.join("themes"));
        config.inventory.plugins = discover_plugins(
            &config_dir.join("plugins"),
            &config.app.enabled_plugins,
            &config.inventory.community_plugin_ids,
        );
        config.inventory.snippets = discover_snippets(&config_dir.join("snippets"));

        Ok(config)
    }
}

#[derive(Debug, Deserialize)]
struct TypesFile {
    #[serde(default)]
    types: BTreeMap<String, String>,
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ObsidianConfigError> {
    let bytes = std::fs::read(path).map_err(|source| ObsidianConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| ObsidianConfigError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn discover_themes(themes_dir: &Path) -> Vec<ThemeInfo> {
    let Ok(entries) = std::fs::read_dir(themes_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
            continue;
        };
        let manifest_path = path.join("manifest.json");
        let (display_name, version, author) = match std::fs::read(&manifest_path) {
            Ok(bytes) => match serde_json::from_slice::<ThemeManifest>(&bytes) {
                Ok(m) => (m.name, m.version, m.author),
                Err(_) => (None, None, None),
            },
            Err(_) => (None, None, None),
        };
        out.push(ThemeInfo {
            name,
            path,
            display_name,
            version,
            author,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn discover_plugins(
    plugins_dir: &Path,
    enabled_map: &BTreeMap<String, bool>,
    community_ids: &[String],
) -> Vec<PluginInfo> {
    let Ok(entries) = std::fs::read_dir(plugins_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(id) = path.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
            continue;
        };
        let manifest_path = path.join("manifest.json");
        let (display_name, version, description, min_app_version) =
            match std::fs::read(&manifest_path) {
                Ok(bytes) => match serde_json::from_slice::<PluginManifest>(&bytes) {
                    Ok(m) => (m.name, m.version, m.description, m.min_app_version),
                    Err(_) => (None, None, None, None),
                },
                Err(_) => (None, None, None, None),
            };
        // A plugin is "enabled" if it's in app.json's enabled_plugins
        // map (with value true) AND it's in community-plugins.json.
        // Core plugins are tracked separately and don't appear here.
        let enabled = enabled_map.get(&id).copied().unwrap_or(false)
            && community_ids.iter().any(|c| c == &id);
        out.push(PluginInfo {
            id,
            path,
            display_name,
            version,
            description,
            min_app_version,
            enabled,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn discover_snippets(snippets_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(snippets_dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("css") {
                p.file_name()?.to_str().map(str::to_string)
            } else {
                None
            }
        })
        .collect();
    out.sort();
    out
}

/// Sentinel used by tests to ensure [`OBSIDIAN_CONFIG_DIR`] is what we
/// think it is.
#[allow(dead_code)]
pub(crate) const _OBSIDIAN_CONFIG_DIR_CHECK: &str = OBSIDIAN_CONFIG_DIR;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_temp_dir(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("tungsten-obs-test-{label}-{pid}-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn build_minimal_vault() -> (PathBuf, PathBuf) {
        let root = unique_temp_dir("minimal");
        let obs = root.join(".obsidian");
        fs::create_dir(&obs).unwrap();
        fs::write(obs.join("app.json"), r#"{"alwaysUpdateLinks": true, "vimMode": true}"#).unwrap();
        fs::write(
            obs.join("appearance.json"),
            r##"{"cssTheme": "Minimal", "baseFontSize": 18, "accentColor": "#abcdef"}"##,
        )
        .unwrap();
        (root, obs)
    }

    #[test]
    fn loads_app_and_appearance() {
        let (root, obs) = build_minimal_vault();
        let cfg = ObsidianConfig::load(&obs).unwrap();
        assert!(cfg.app.always_update_links);
        assert!(cfg.app.vim_mode);
        assert_eq!(cfg.appearance.css_theme.as_deref(), Some("Minimal"));
        assert_eq!(cfg.appearance.base_font_size, Some(18));
        assert_eq!(cfg.appearance.accent_color.as_deref(), Some("#abcdef"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_appearance_defaults_to_empty() {
        let root = unique_temp_dir("no-appearance");
        let obs = root.join(".obsidian");
        fs::create_dir(&obs).unwrap();
        fs::write(obs.join("app.json"), "{}").unwrap();
        let cfg = ObsidianConfig::load(&obs).unwrap();
        assert_eq!(cfg.appearance, AppearanceConfig::default());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn discovers_themes_with_manifest_metadata() {
        let (root, obs) = build_minimal_vault();
        let theme = obs.join("themes/Minimal");
        fs::create_dir_all(&theme).unwrap();
        fs::write(
            theme.join("manifest.json"),
            r#"{"name": "Minimal", "version": "8.2.1", "author": "@kepano"}"#,
        )
        .unwrap();
        fs::write(theme.join("theme.css"), "/* css */").unwrap();

        let cfg = ObsidianConfig::load(&obs).unwrap();
        assert_eq!(cfg.inventory.themes.len(), 1);
        let t = &cfg.inventory.themes[0];
        assert_eq!(t.name, "Minimal");
        assert_eq!(t.display_name.as_deref(), Some("Minimal"));
        assert_eq!(t.version.as_deref(), Some("8.2.1"));
        assert_eq!(t.author.as_deref(), Some("@kepano"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn discovers_plugins_and_marks_enabled_correctly() {
        let (root, obs) = build_minimal_vault();
        let plugin = obs.join("plugins/obsidian-tasks");
        fs::create_dir_all(&plugin).unwrap();
        fs::write(
            plugin.join("manifest.json"),
            r#"{"id": "obsidian-tasks", "name": "Tasks", "version": "8.2.2",
                "description": "Track tasks.", "minAppVersion": "1.8.7"}"#,
        )
        .unwrap();

        // enabled_plugins is per-plugin in app.json
        let app: serde_json::Value = serde_json::from_str(&fs::read_to_string(obs.join("app.json")).unwrap()).unwrap();
        let mut app = app.as_object().unwrap().clone();
        app.insert(
            "enabledPlugins".into(),
            serde_json::json!({ "obsidian-tasks": true }),
        );
        fs::write(
            obs.join("app.json"),
            serde_json::to_string_pretty(&app).unwrap(),
        )
        .unwrap();

        // community-plugins.json
        fs::write(obs.join("community-plugins.json"), r#"["obsidian-tasks"]"#).unwrap();

        let cfg = ObsidianConfig::load(&obs).unwrap();
        assert_eq!(cfg.inventory.community_plugin_ids, vec!["obsidian-tasks"]);
        assert_eq!(cfg.inventory.plugins.len(), 1);
        let p = &cfg.inventory.plugins[0];
        assert_eq!(p.id, "obsidian-tasks");
        assert_eq!(p.display_name.as_deref(), Some("Tasks"));
        assert!(p.enabled, "should be marked enabled when in both lists");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn plugin_not_enabled_if_only_in_community_list() {
        let (root, obs) = build_minimal_vault();
        let plugin = obs.join("plugins/obsidian-tasks");
        fs::create_dir_all(&plugin).unwrap();
        fs::write(plugin.join("manifest.json"), r#"{"id": "obsidian-tasks"}"#).unwrap();
        fs::write(obs.join("community-plugins.json"), r#"["obsidian-tasks"]"#).unwrap();
        // app.json has no enabledPlugins entry

        let cfg = ObsidianConfig::load(&obs).unwrap();
        assert!(!cfg.inventory.plugins[0].enabled);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn discovers_snippets() {
        let (root, obs) = build_minimal_vault();
        fs::create_dir(obs.join("snippets")).unwrap();
        fs::write(obs.join("snippets/a.css"), "").unwrap();
        fs::write(obs.join("snippets/b.css"), "").unwrap();
        fs::write(obs.join("snippets/not-css.txt"), "").unwrap();

        let cfg = ObsidianConfig::load(&obs).unwrap();
        assert_eq!(cfg.inventory.snippets, vec!["a.css", "b.css"]);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn loads_types_file() {
        let (root, obs) = build_minimal_vault();
        fs::write(
            obs.join("types.json"),
            r#"{ "types": { "aliases": "aliases", "cssclasses": "multitext" } }"#,
        )
        .unwrap();

        let cfg = ObsidianConfig::load(&obs).unwrap();
        assert_eq!(cfg.types.get("aliases").map(String::as_str), Some("aliases"));
        assert_eq!(
            cfg.types.get("cssclasses").map(String::as_str),
            Some("multitext")
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn errors_when_config_dir_missing() {
        let dir = unique_temp_dir("missing");
        let bogus = dir.join("not-here");
        let err = ObsidianConfig::load(&bogus).unwrap_err();
        matches!(err, ObsidianConfigError::MissingConfigDir(_));
        fs::remove_dir_all(&dir).ok();
    }
}
