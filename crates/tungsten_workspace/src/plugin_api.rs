//! Plugin manifest and registry types (M3.x foundation).
//!
//! Obsidian's plugin ecosystem stores each plugin in
//! `.obsidian/plugins/<id>/`, with at minimum:
//!
//! - `manifest.json` — plugin metadata (id, name, version,
//!   author, min app version, description, isDesktopOnly)
//! - `main.js` — the plugin code
//!
//! The vault-level `community-plugins.json` is an array of
//! enabled plugin ids. Tungsten's compat layer reads both.
//!
//! This module does **not** execute plugin code (that
//! requires a JS runtime — see M3.1+). It only parses
//! manifests and tracks the registry, so the rest of the
//! codebase can reason about which plugins are installed,
//! enabled, and which versions are present.
//!
//! # manifest.json format
//!
//! ```json
//! {
//!   "id": "dataview",
//!   "name": "Dataview",
//!   "version": "0.5.55",
//!   "minAppVersion": "1.1.0",
//!   "description": "Query your notes with a SQL-like language",
//!   "author": "Michael Brenan",
//!   "isDesktopOnly": false
//! }
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// A parsed plugin manifest. The shape mirrors Obsidian's
/// `manifest.json` plus a few fields Tungsten adds for
/// internal tracking.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PluginManifest {
    /// Plugin id (lowercase, unique within the vault).
    pub id: String,
    /// Display name shown in the settings panel.
    pub name: String,
    /// Plugin version. Compared lexically — semver is the
    /// recommended shape but we don't enforce it.
    pub version: String,
    /// Minimum Obsidian (or Tungsten) version required.
    #[serde(rename = "minAppVersion", default)]
    pub min_app_version: String,
    /// Short description shown in the plugin list.
    #[serde(default)]
    pub description: String,
    /// Author name. May be a free-form string.
    #[serde(default)]
    pub author: String,
    /// Whether the plugin requires desktop-only APIs.
    #[serde(rename = "isDesktopOnly", default)]
    pub is_desktop_only: bool,
    /// Tungsten extension: an opaque compatibility hint
    /// (`"native"`, `"polyfill"`, `"unsupported"`). Absent
    /// in stock Obsidian manifests.
    #[serde(default, rename = "tungstenCompat")]
    pub tungsten_compat: Option<String>,
}

/// Parse a `manifest.json` from raw bytes.
pub fn parse_manifest(bytes: &[u8]) -> Result<PluginManifest, serde_json::Error> {
    serde_json::from_slice(bytes)
}

/// A registry of installed plugins keyed by id.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PluginRegistry {
    plugins: BTreeMap<String, InstalledPlugin>,
}

/// One installed plugin: its manifest plus its on-disk
/// location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPlugin {
    pub manifest: PluginManifest,
    /// Absolute path to the plugin folder.
    pub path: std::path::PathBuf,
    /// Whether the plugin is in the `community-plugins.json`
    /// enabled list.
    pub enabled: bool,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or update a plugin. Replaces any previous
    /// entry with the same id.
    pub fn insert(&mut self, plugin: InstalledPlugin) {
        self.plugins.insert(plugin.manifest.id.clone(), plugin);
    }

    /// Get a plugin by id.
    pub fn get(&self, id: &str) -> Option<&InstalledPlugin> {
        self.plugins.get(id)
    }

    /// All plugins, sorted by id.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &InstalledPlugin)> {
        self.plugins.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// All enabled plugins.
    pub fn enabled(&self) -> impl Iterator<Item = &InstalledPlugin> {
        self.plugins.values().filter(|p| p.enabled)
    }

    /// All disabled plugins.
    pub fn disabled(&self) -> impl Iterator<Item = &InstalledPlugin> {
        self.plugins.values().filter(|p| !p.enabled)
    }

    /// Number of installed plugins (enabled + disabled).
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// True if the registry has no plugins.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Set the enabled state of a plugin by id. Returns
    /// `true` if the plugin was found.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        if let Some(p) = self.plugins.get_mut(id) {
            p.enabled = enabled;
            true
        } else {
            false
        }
    }
}

/// Discover installed plugins by walking
/// `.obsidian/plugins/<id>/manifest.json`.
///
/// The `enabled_ids` argument is the parsed
/// `community-plugins.json` array. Each plugin's
/// `enabled` flag is set to `true` iff its id is in this
/// list.
pub fn discover(
    obsidian_dir: &Path,
    enabled_ids: &[String],
) -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    let plugins_dir = obsidian_dir.join("plugins");
    let Ok(entries) = std::fs::read_dir(&plugins_dir) else {
        return registry;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("manifest.json");
        if !manifest_path.is_file() {
            continue;
        }
        let Ok(bytes) = std::fs::read(&manifest_path) else {
            continue;
        };
        let Ok(manifest) = parse_manifest(&bytes) else {
            continue;
        };
        let enabled = enabled_ids.iter().any(|id| id == &manifest.id);
        registry.insert(InstalledPlugin {
            manifest,
            path,
            enabled,
        });
    }
    registry
}

/// Parse the vault's `community-plugins.json` (a JSON array
/// of enabled plugin id strings).
pub fn parse_enabled_list(
    obsidian_dir: &Path,
) -> Result<Vec<String>, std::io::Error> {
    let path = obsidian_dir.join("community-plugins.json");
    let bytes = std::fs::read(&path)?;
    serde_json::from_slice::<Vec<String>>(&bytes).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
    })
}

/// A typed description of a method on the `obsidian` module
/// shim. The compat layer in M3.1+ will map these to
/// native function calls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShimMethod {
    /// JS-side name (e.g. `"app.workspace.openLinkText"`).
    pub js_name: String,
    /// Rust-side function name (e.g. `"open_link_text"`).
    pub rust_name: String,
    /// One-line description for docs.
    pub description: String,
    /// Whether the method requires desktop-only APIs.
    pub desktop_only: bool,
}

/// The full surface area of the `obsidian` module shim,
/// hard-coded. As Tungsten grows, this list grows with it.
pub fn shim_surface() -> Vec<ShimMethod> {
    vec![
        // Workspace
        shim("app.workspace.openLinkText", "open_link_text", "Open a wikilink in a new leaf", false),
        shim("app.workspace.getLeaf", "get_leaf", "Get a leaf by id or kind", false),
        shim("app.workspace.getLeavesOfType", "get_leaves_of_type", "List leaves of a given type", false),
        shim("app.workspace.revealLeaf", "reveal_leaf", "Reveal a leaf in the UI", false),
        shim("app.workspace.activeLeaf", "active_leaf", "The currently-focused leaf", false),
        // Vault
        shim("app.vault.adapter.read", "vault_read", "Read a file's contents", false),
        shim("app.vault.adapter.write", "vault_write", "Write a file's contents", false),
        shim("app.vault.create", "vault_create", "Create a new file", false),
        shim("app.vault.delete", "vault_delete", "Delete a file", false),
        shim("app.vault.rename", "vault_rename", "Rename a file", false),
        shim("app.vault.getAbstractFileByPath", "vault_get_by_path", "Look up a file by path", false),
        shim("app.vault.cachedRead", "vault_cached_read", "Read a file with cache", false),
        // Metadata cache
        shim("app.metadataCache.getFileCache", "metadata_cache_get", "Get the parsed cache for a file", false),
        shim("app.metadataCache.on", "metadata_cache_on", "Subscribe to cache events", false),
        // File manager
        shim("app.fileManager.generateMarkdownLink", "file_manager_md_link", "Build a wikilink/markdown link", false),
        shim("app.fileManager.processFrontMatter", "file_manager_process_fm", "Edit frontmatter safely", false),
        // Plugin base
        shim("Plugin.loadData", "plugin_load_data", "Read the plugin's settings JSON", false),
        shim("Plugin.saveData", "plugin_save_data", "Write the plugin's settings JSON", false),
        // Editor
        shim("Editor.getValue", "editor_get_value", "Read the editor's content", false),
        shim("Editor.setValue", "editor_set_value", "Replace the editor's content", false),
        shim("Editor.replaceRange", "editor_replace_range", "Replace a range of text", false),
    ]
}

fn shim(
    js_name: &str,
    rust_name: &str,
    description: &str,
    desktop_only: bool,
) -> ShimMethod {
    ShimMethod {
        js_name: js_name.into(),
        rust_name: rust_name.into(),
        description: description.into(),
        desktop_only,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> std::path::PathBuf {
        let base = std::env::temp_dir();
        let unique = format!(
            "tungsten-plugin-test-{}-{}",
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
    fn parse_manifest_minimal() {
        let s = r#"{
            "id": "x",
            "name": "X",
            "version": "1.0.0"
        }"#;
        let m = parse_manifest(s.as_bytes()).unwrap();
        assert_eq!(m.id, "x");
        assert_eq!(m.version, "1.0.0");
        assert_eq!(m.min_app_version, "");
        assert_eq!(m.author, "");
        assert!(!m.is_desktop_only);
    }

    #[test]
    fn parse_manifest_full() {
        let s = r#"{
            "id": "dataview",
            "name": "Dataview",
            "version": "0.5.55",
            "minAppVersion": "1.1.0",
            "description": "Query notes",
            "author": "Michael Brenan",
            "isDesktopOnly": false,
            "tungstenCompat": "polyfill"
        }"#;
        let m = parse_manifest(s.as_bytes()).unwrap();
        assert_eq!(m.tungsten_compat.as_deref(), Some("polyfill"));
    }

    #[test]
    fn discover_finds_installed_plugins() {
        let dir = tempdir();
        let obs = dir.join(".obsidian");
        let plugins = obs.join("plugins");
        fs::create_dir_all(plugins.join("alpha")).unwrap();
        fs::write(
            plugins.join("alpha/manifest.json"),
            r#"{"id":"alpha","name":"Alpha","version":"0.1.0"}"#,
        )
        .unwrap();
        fs::create_dir_all(plugins.join("beta")).unwrap();
        fs::write(
            plugins.join("beta/manifest.json"),
            r#"{"id":"beta","name":"Beta","version":"2.0.0"}"#,
        )
        .unwrap();
        // gamma has no manifest.
        fs::create_dir_all(plugins.join("gamma")).unwrap();
        fs::write(obs.join("community-plugins.json"), r#"["alpha"]"#).unwrap();
        let enabled = parse_enabled_list(&obs).unwrap();
        let registry = discover(&obs, &enabled);
        assert_eq!(registry.len(), 2);
        assert!(registry.get("alpha").unwrap().enabled);
        assert!(!registry.get("beta").unwrap().enabled);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn discover_handles_missing_dir() {
        let dir = tempdir();
        let obs = dir.join(".obsidian");
        let registry = discover(&obs, &[]);
        assert!(registry.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn shim_surface_includes_basics() {
        let surface = shim_surface();
        let names: Vec<&str> = surface.iter().map(|s| s.js_name.as_str()).collect();
        assert!(names.contains(&"app.workspace.openLinkText"));
        assert!(names.contains(&"app.vault.adapter.read"));
        assert!(names.iter().any(|n| n.starts_with("Editor.")));
        // No duplicates.
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len());
    }

    #[test]
    fn set_enabled_flips_flag() {
        let mut r = PluginRegistry::new();
        r.insert(InstalledPlugin {
            manifest: PluginManifest {
                id: "x".into(),
                name: "X".into(),
                version: "1.0.0".into(),
                min_app_version: "".into(),
                description: "".into(),
                author: "".into(),
                is_desktop_only: false,
                tungsten_compat: None,
            },
            path: std::path::PathBuf::from("/x"),
            enabled: false,
        });
        assert!(r.set_enabled("x", true));
        assert!(r.get("x").unwrap().enabled);
        assert!(!r.set_enabled("missing", true));
    }
}
