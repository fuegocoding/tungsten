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
//! templates, search) on top of the [`Vault`] type defined here.

use std::path::{Path, PathBuf};

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
}
