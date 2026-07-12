//! Multi-vault workspace manager.
//!
//! The main `tungsten` binary holds at most one of these at a time
//! (in `AppState`). It tracks:
//! - which vaults are open (by canonicalized root path),
//! - which one is currently active (the "focused" vault, used for
//!   backlinks, search, the status-bar vault name, etc.),
//! - the cached [`ObsidianConfig`] for each open vault, loaded lazily
//!   on first access.
//!
//! The struct is intentionally small and synchronous: opening and
//! closing a vault is a fast in-memory operation. The expensive part
//! — reading the full vault contents, indexing notes, building the
//! link graph — is the M1.x knowledge-layer work and lives in
//! separate crates (not yet implemented).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::obsidian_config::{ObsidianConfig, ObsidianConfigError};
use crate::Vault;

/// Errors that can occur while managing the workspace.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("path is not a vault (no .obsidian/ directory): {0}")]
    NotAVault(PathBuf),
    #[error("vault already open: {0}")]
    AlreadyOpen(PathBuf),
    #[error("vault not found: {0}")]
    NotFound(PathBuf),
    #[error("config error: {0}")]
    Config(#[from] ObsidianConfigError),
}

/// One open vault: the [`Vault`] itself plus its lazily-loaded config.
#[derive(Debug)]
struct OpenVault {
    vault: Vault,
    config: Option<ObsidianConfig>,
}

/// A multi-vault workspace manager.
///
/// Construct with [`TungstenWorkspace::new`]. Open vaults with
/// [`open`](Self::open) or [`open_detected`](Self::open_detected) (the
/// latter walks up looking for `.obsidian/`). Switch the active
/// vault with [`set_active`](Self::set_active). Access config with
/// [`config`](Self::config).
#[derive(Debug, Default)]
pub struct TungstenWorkspace {
    vaults: BTreeMap<PathBuf, OpenVault>,
    active: Option<PathBuf>,
}

impl TungstenWorkspace {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open `path` as a vault. Fails if `path` does not contain a
    /// `.obsidian/` directory. Becomes the active vault if no other
    /// vault is active.
    pub fn open(&mut self, path: &Path) -> Result<&Vault, WorkspaceError> {
        let vault = Vault::open(path).ok_or_else(|| WorkspaceError::NotAVault(path.to_path_buf()))?;
        self.open_vault_inner(vault, false)
    }

    /// Detect a vault by walking up from `start` and open it. Fails
    /// if no `.obsidian/` is found in any ancestor.
    pub fn open_detected(&mut self, start: &Path) -> Result<&Vault, WorkspaceError> {
        let vault = Vault::detect(start).ok_or_else(|| WorkspaceError::NotAVault(start.to_path_buf()))?;
        self.open_vault_inner(vault, false)
    }

    /// Register an already-detected [`Vault`] (no filesystem check).
    /// `set_active_if_none` controls whether to set this vault as
    /// active when no vault is currently active.
    pub fn open_vault(&mut self, vault: Vault, set_active_if_none: bool) -> Result<&Vault, WorkspaceError> {
        self.open_vault_inner(vault, set_active_if_none)
    }

    fn open_vault_inner(
        &mut self,
        vault: Vault,
        set_active_if_none: bool,
    ) -> Result<&Vault, WorkspaceError> {
        let key = canonicalize_or_original(vault.root());
        if self.vaults.contains_key(&key) {
            return Err(WorkspaceError::AlreadyOpen(key));
        }
        if self.active.is_none() || set_active_if_none {
            self.active = Some(key.clone());
        }
        let prev = self.vaults.insert(
            key.clone(),
            OpenVault {
                vault,
                config: None,
            },
        );
        debug_assert!(prev.is_none());
        Ok(&self.vaults.get(&key).unwrap().vault)
    }

    /// Close the vault at `path` (canonicalized). Returns `true` if
    /// a vault was actually closed; `false` if it wasn't open.
    pub fn close(&mut self, path: &Path) -> bool {
        let key = canonicalize_or_original(path);
        if self.vaults.remove(&key).is_some() {
            if self.active.as_ref() == Some(&key) {
                self.active = self.vaults.keys().next().cloned();
            }
            true
        } else {
            false
        }
    }

    /// Set the active vault. Returns `true` if the vault was open and
    /// is now active; `false` if it wasn't open.
    pub fn set_active(&mut self, path: &Path) -> bool {
        let key = canonicalize_or_original(path);
        if self.vaults.contains_key(&key) {
            self.active = Some(key);
            true
        } else {
            false
        }
    }

    /// The active vault, if any.
    pub fn active(&self) -> Option<&Vault> {
        self.active.as_ref().and_then(|k| self.vaults.get(k).map(|o| &o.vault))
    }

    /// Borrow an open vault by path.
    pub fn get(&self, path: &Path) -> Option<&Vault> {
        let key = canonicalize_or_original(path);
        self.vaults.get(&key).map(|o| &o.vault)
    }

    /// All open vaults, in deterministic (path) order.
    pub fn vaults(&self) -> impl Iterator<Item = &Vault> {
        self.vaults.values().map(|o| &o.vault)
    }

    /// How many vaults are open.
    pub fn len(&self) -> usize {
        self.vaults.len()
    }

    /// Whether the workspace has no open vaults.
    pub fn is_empty(&self) -> bool {
        self.vaults.is_empty()
    }

    /// Load (lazily) and return the [`ObsidianConfig`] for the vault
    /// at `path`. Subsequent calls return the cached config.
    pub fn config(&mut self, path: &Path) -> Result<&ObsidianConfig, WorkspaceError> {
        let key = canonicalize_or_original(path);
        let entry = self
            .vaults
            .get_mut(&key)
            .ok_or_else(|| WorkspaceError::NotFound(key.clone()))?;
        if entry.config.is_none() {
            let cfg = ObsidianConfig::load(entry.vault.config_dir())?;
            entry.config = Some(cfg);
        }
        Ok(entry.config.as_ref().unwrap())
    }
}

/// Best-effort path canonicalization for vault identity. If the path
/// doesn't exist (rare; only happens in tests with deleted dirs) or
/// canonicalize fails, we fall back to the original path. The
/// canonical form is what disambiguates "the same vault opened via
/// different relative paths."
fn canonicalize_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_temp_dir(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("tungsten-ws-test-{label}-{pid}-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_vault(label: &str) -> PathBuf {
        let dir = unique_temp_dir(label);
        fs::create_dir(dir.join(".obsidian")).unwrap();
        dir
    }

    #[test]
    fn new_is_empty() {
        let ws = TungstenWorkspace::new();
        assert!(ws.is_empty());
        assert_eq!(ws.len(), 0);
        assert!(ws.active().is_none());
        assert_eq!(ws.vaults().count(), 0);
    }

    #[test]
    fn open_succeeds_on_vault_root() {
        let dir = make_vault("open-root");
        let mut ws = TungstenWorkspace::new();
        let v = ws.open(&dir).unwrap();
        assert_eq!(v.root(), dir);
        assert_eq!(ws.len(), 1);
        assert_eq!(ws.active().unwrap().root(), dir);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn open_fails_on_non_vault() {
        let dir = unique_temp_dir("not-a-vault");
        let mut ws = TungstenWorkspace::new();
        let err = ws.open(&dir).unwrap_err();
        assert!(matches!(err, WorkspaceError::NotAVault(_)));
        assert!(ws.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn open_detected_finds_vault_up_the_tree() {
        let root = make_vault("detect-ws");
        let nested = root.join("Journal/2026");
        fs::create_dir_all(&nested).unwrap();
        let note = nested.join("2026-07-12.md");
        fs::write(&note, "").unwrap();
        let mut ws = TungstenWorkspace::new();
        let v = ws.open_detected(&note).unwrap();
        assert_eq!(v.root(), root);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn open_detected_fails_when_no_vault_in_tree() {
        let dir = unique_temp_dir("no-vault-ws");
        let nested = dir.join("a/b/c.md");
        fs::create_dir_all(nested.parent().unwrap()).unwrap();
        fs::write(&nested, "").unwrap();
        let mut ws = TungstenWorkspace::new();
        let err = ws.open_detected(&nested).unwrap_err();
        assert!(matches!(err, WorkspaceError::NotAVault(_)));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn duplicate_open_fails() {
        let dir = make_vault("dup-open");
        let mut ws = TungstenWorkspace::new();
        ws.open(&dir).unwrap();
        let err = ws.open(&dir).unwrap_err();
        assert!(matches!(err, WorkspaceError::AlreadyOpen(_)));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn first_open_becomes_active_others_do_not() {
        let a = make_vault("a");
        let b = make_vault("b");
        let mut ws = TungstenWorkspace::new();
        ws.open(&a).unwrap();
        ws.open(&b).unwrap();
        assert_eq!(ws.active().unwrap().root(), a, "first open wins as active");
        fs::remove_dir_all(&a).ok();
        fs::remove_dir_all(&b).ok();
    }

    #[test]
    fn set_active_switches_focus() {
        let a = make_vault("set-active-a");
        let b = make_vault("set-active-b");
        let mut ws = TungstenWorkspace::new();
        ws.open(&a).unwrap();
        ws.open(&b).unwrap();
        assert!(ws.set_active(&b));
        assert_eq!(ws.active().unwrap().root(), b);
        assert!(!ws.set_active(&unique_temp_dir("never-opened")));
        fs::remove_dir_all(&a).ok();
        fs::remove_dir_all(&b).ok();
    }

    #[test]
    fn close_drops_vault_and_picks_replacement_active() {
        let a = make_vault("close-a");
        let b = make_vault("close-b");
        let mut ws = TungstenWorkspace::new();
        ws.open(&a).unwrap();
        ws.open(&b).unwrap();
        // a is active
        assert_eq!(ws.active().unwrap().root(), a);
        // close active vault; some other vault should become active
        assert!(ws.close(&a));
        assert!(ws.active().is_some());
        assert_eq!(ws.active().unwrap().root(), b);
        // closing a non-open vault returns false
        assert!(!ws.close(&a));
        fs::remove_dir_all(&a).ok();
        fs::remove_dir_all(&b).ok();
    }

    #[test]
    fn config_loads_lazily_and_caches() {
        let dir = make_vault("config-cache");
        let obs = dir.join(".obsidian");
        fs::write(obs.join("app.json"), r#"{"vimMode": true}"#).unwrap();
        fs::write(obs.join("appearance.json"), r#"{"cssTheme": "Test"}"#).unwrap();

        let mut ws = TungstenWorkspace::new();
        ws.open(&dir).unwrap();
        let cfg1 = ws.config(&dir).unwrap().clone();
        let cfg2 = ws.config(&dir).unwrap().clone();
        assert!(cfg1.app.vim_mode);
        assert_eq!(cfg1.appearance.css_theme.as_deref(), Some("Test"));
        // Cache check: the same Arc/pointer or same value works; here we
        // rely on the struct returning the same data both times.
        assert_eq!(cfg1, cfg2);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_on_closed_vault_errors() {
        let dir = make_vault("cfg-closed");
        let mut ws = TungstenWorkspace::new();
        ws.open(&dir).unwrap();
        ws.close(&dir);
        let err = ws.config(&dir).unwrap_err();
        assert!(matches!(err, WorkspaceError::NotFound(_)));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn open_vault_via_existing_value() {
        let dir = make_vault("open-value");
        let vault = Vault::open(&dir).unwrap();
        let mut ws = TungstenWorkspace::new();
        let v = ws.open_vault(vault, true).unwrap();
        assert_eq!(v.root(), dir);
        assert_eq!(ws.active().unwrap().root(), dir);
        fs::remove_dir_all(&dir).ok();
    }
}
