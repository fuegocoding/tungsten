//! Tungsten workspace — the vault concept and the knowledge layer.
//!
//! Tungsten organizes user content into **vaults**: folders on disk
//! containing a `.obsidian/` config directory. A vault is the unit of
//! everything in Tungsten:
//!
//! - A vault is a folder of plain Markdown files (`.md`) and attachments
//!   (images, PDFs, audio, etc.).
//! - A vault's settings, themes, plugins, hotkeys, snippets, and layout
//!   state live in `.obsidian/`. Tungsten reads and writes this directory
//!   so existing Obsidian vaults open without conversion.
//! - A vault can be opened in one of several "modes" — code editing
//!   (the default Zed substrate) or note-taking (Tungsten's
//!   first-class mode, M0.2 in the roadmap). The mode is auto-detected
//!   from the presence of `.obsidian/`.
//!
//! This crate is the foundation. The M1.x milestones layer the rest of
//! the knowledge management (graph, tags, backlinks, daily notes,
//! templates, search) on top of the [`Vault`] and [`ObsidianConfig`]
//! types defined here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub mod note;
pub mod obsidian_config;
pub mod index_db;
pub use index_db::{IndexDbError, SCHEMA_VERSION};
pub mod rename;
pub use rename::{RenameError, RenameResult, rewrite_links_in_content};

pub mod attachments;
pub use attachments::{mime_from_extension, mime_from_filename, Attachment, AttachmentIndex, AttachmentKind};

pub mod dql;
pub use dql::{
    execute as dql_execute, parse_query as dql_parse_query, tokenize as dql_tokenize, CompareOp,
    DqlError, DqlNoteRow, DqlQuery, DqlResult, DqlRow, FromClause, Ident, Keyword, Literal,
    SourceType, Token, WhereClause, WhereNode,
};

pub use note::{Link, LinkKind, Note, UnlinkedMention};
pub use obsidian_config::{
    AppearanceConfig, AppConfig, NewLinkFormat, ObsidianConfig, ObsidianConfigError, PluginInfo,
    ThemeInfo,
};

pub mod index;
pub use index::{IndexError, IndexStats, NoteIndex, IGNORED_DIRS};

pub mod notes_io;
pub use notes_io::{
    render_template, NoteCreateError, NoteCreator, TemplateVars,
};

pub mod search;
pub use search::{parse_search_query, PropertyFilter, SearchError, SearchMatch, SearchQuery, SearchResult};

mod note_parser;
mod workspace;
pub use workspace::{TungstenWorkspace, WorkspaceError};

/// The name of the per-vault configuration directory.
///
/// Obsidian uses `.obsidian/` (lowercase, dotfile). Tungsten accepts
/// `.obsidian/` for compatibility with existing vaults; a future
/// `tungsten/` namespace may be added for Tungsten-specific config.
pub const OBSIDIAN_CONFIG_DIR: &str = ".obsidian";

/// A detected Tungsten vault on disk.
///
/// A vault is a folder containing a `.obsidian/` config directory.
/// Construct one with [`Vault::detect`] (which walks up from a given
/// path looking for the config directory) or [`Vault::open`] (which
/// requires the path to *be* the vault root).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vault {
    root: PathBuf,
    config_dir: PathBuf,
}

impl Vault {
    /// The path to the vault root (the folder containing `.obsidian/`).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The path to the vault's config directory (`.obsidian/`).
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// A short name for the vault, derived from the root folder name.
    pub fn name(&self) -> String {
        self.root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("vault")
            .to_string()
    }

    /// Load the vault's `.obsidian/` configuration (appearance, themes,
    /// plugins catalog, snippets, etc.). Returns an error if any
    /// required file is malformed; missing optional files are skipped.
    pub fn load_config(&self) -> Result<ObsidianConfig, ObsidianConfigError> {
        ObsidianConfig::load(self.config_dir())
    }

    /// Detect a vault by walking up from `start` looking for a
    /// `.obsidian/` directory. Returns the *innermost* vault root found,
    /// which matters when a vault is nested inside another vault (rare
    /// but supported by Obsidian).
    pub fn detect(start: &Path) -> Option<Self> {
        let mut current = start;
        loop {
            if let Some(vault) = Self::open(current) {
                return Some(vault);
            }
            match current.parent() {
                Some(parent) if parent != current => current = parent,
                _ => return None,
            }
        }
    }

    /// Try to open `path` as a vault root. Returns `Some` iff `path`
    /// contains a `.obsidian/` directory.
    pub fn open(path: &Path) -> Option<Self> {
        let config_dir = path.join(OBSIDIAN_CONFIG_DIR);
        if config_dir.is_dir() {
            Some(Self {
                root: path.to_path_buf(),
                config_dir,
            })
        } else {
            None
        }
    }
}

/// Combined map of a vault's resources (themes, plugins, snippets),
/// used by the loader to surface what's available without forcing the
/// caller to walk the directory tree themselves.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct VaultInventory {
    pub themes: Vec<ThemeInfo>,
    pub plugins: Vec<PluginInfo>,
    pub snippets: Vec<String>,
    pub community_plugin_ids: Vec<String>,
    pub core_plugins: BTreeMap<String, bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Build a unique temp dir under the system temp dir, then return it.
    /// The directory is created and is the caller's responsibility to
    /// remove (we keep tests hermetic and don't actually touch user data).
    fn unique_temp_dir(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("tungsten-vault-test-{label}-{pid}-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn open_returns_none_when_no_obsidian_dir() {
        let dir = unique_temp_dir("no-obsidian");
        assert!(Vault::open(&dir).is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn open_returns_some_when_obsidian_dir_exists() {
        let dir = unique_temp_dir("with-obsidian");
        fs::create_dir(dir.join(".obsidian")).unwrap();
        let vault = Vault::open(&dir).expect("vault should open");
        assert_eq!(vault.root(), dir);
        assert_eq!(vault.config_dir(), dir.join(".obsidian"));
        assert!(!vault.name().is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detect_finds_vault_from_nested_file() {
        let vault_root = unique_temp_dir("nested");
        let nested = vault_root.join("daily/2026-07-12.md");
        fs::create_dir_all(nested.parent().unwrap()).unwrap();
        fs::write(&nested, "# hello\n").unwrap();
        fs::create_dir(vault_root.join(".obsidian")).unwrap();

        let found = Vault::detect(&nested).expect("detect should find vault");
        assert_eq!(found.root(), vault_root);

        fs::remove_dir_all(&vault_root).ok();
    }

    #[test]
    fn detect_returns_none_when_no_vault_anywhere() {
        let dir = unique_temp_dir("no-vault");
        let nested = dir.join("a/b/c.md");
        fs::create_dir_all(nested.parent().unwrap()).unwrap();
        fs::write(&nested, "").unwrap();
        assert!(Vault::detect(&nested).is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detect_finds_innermost_vault_when_nested() {
        // /outer/.obsidian/         ← outer vault
        // /outer/inner/.obsidian/   ← inner vault (wins)
        // /outer/inner/notes/x.md
        let outer = unique_temp_dir("outer-vault");
        let inner = outer.join("inner");
        let note = inner.join("notes/x.md");
        fs::create_dir_all(note.parent().unwrap()).unwrap();
        fs::write(&note, "").unwrap();
        fs::create_dir(outer.join(".obsidian")).unwrap();
        fs::create_dir(inner.join(".obsidian")).unwrap();

        let found = Vault::detect(&note).expect("detect");
        assert_eq!(found.root(), inner, "innermost vault wins");

        fs::remove_dir_all(&outer).ok();
    }

    #[test]
    fn name_uses_folder_basename() {
        let dir = unique_temp_dir("name-check");
        fs::create_dir(dir.join(".obsidian")).unwrap();
        let vault = Vault::open(&dir).unwrap();
        let expected = dir.file_name().unwrap().to_str().unwrap().to_string();
        assert_eq!(vault.name(), expected);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_config_on_minimal_vault() {
        let dir = unique_temp_dir("load-config");
        let obsidian = dir.join(".obsidian");
        fs::create_dir(&obsidian).unwrap();
        fs::write(
            obsidian.join("appearance.json"),
            r##"{"cssTheme": "Minimal", "baseFontSize": 16, "accentColor": "#ff0000"}"##,
        )
        .unwrap();
        fs::write(obsidian.join("app.json"), r#"{"alwaysUpdateLinks": true}"#).unwrap();

        let vault = Vault::open(&dir).unwrap();
        let cfg = vault.load_config().expect("load_config");
        assert_eq!(cfg.appearance.css_theme.as_deref(), Some("Minimal"));
        assert_eq!(cfg.appearance.base_font_size, Some(16));
        assert_eq!(cfg.appearance.accent_color.as_deref(), Some("#ff0000"));
        assert!(cfg.app.always_update_links);

        fs::remove_dir_all(&dir).ok();
    }
}
