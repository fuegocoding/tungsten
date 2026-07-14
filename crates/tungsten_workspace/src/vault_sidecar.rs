//! Vault sidecar state.
//!
//! Tungsten stores per-vault state in a `.tungsten/` directory at
//! the vault root. The first file written there is
//! `state.json`, a small JSON document that records:
//!
//! - the vault's display name and root path,
//! - the last time Tungsten opened this vault,
//! - the Tungsten version that opened it.
//!
//! Future state (panel visibility defaults, the index DB, cached
//! settings, the welcome-tab-seen flag) will live alongside this
//! file. Sidecar paths:
//!
//! - `.tungsten/state.json`   — this file
//! - `.tungsten/index.db`     — the SQLite-backed note index (M1.2)
//! - `.tungsten/snippets/`   — vault-local CSS snippets
//! - `.tungsten/themes/`     — vault-local themes
//! - `.tungsten/plugins/`    — vault-local native extension state
//!
//! All paths under `.tungsten/` are skipped by [`crate::index::IGNORED_DIRS`]
//! so the sidecar state never shows up in the note index or the
//! search results.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Vault;

/// The JSON shape of `.tungsten/state.json`. The file is read on
/// startup and updated when the vault is opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultState {
    /// Schema version of this file. Bumped on breaking changes
    /// to the schema.
    pub schema_version: u32,
    /// The vault's display name (the folder basename).
    pub vault_name: String,
    /// Absolute path to the vault root.
    pub vault_root: PathBuf,
    /// Unix seconds at which the vault was last opened.
    pub last_opened_at: u64,
    /// The Tungsten version that last opened this vault. Stored
    /// as the crate version; the binary's version is also
    /// available but the crate version is what the format
    /// actually depends on.
    pub opened_by: String,
}

/// Current schema version. Bumped on any breaking change to
/// `VaultState` (added/removed/renamed fields, changed semantics).
pub const STATE_SCHEMA_VERSION: u32 = 1;

/// Ensure the `.tungsten/` sidecar directory exists. Creates it
/// (and any missing parents) if needed. Idempotent.
pub fn ensure_sidecar_dir(vault_root: &Path) -> std::io::Result<PathBuf> {
    let sidecar = vault_root.join(".tungsten");
    std::fs::create_dir_all(&sidecar)?;
    Ok(sidecar)
}

/// Write (or overwrite) `.tungsten/state.json` for the given
/// vault, recording `last_opened_at` as the current time and
/// `opened_by` as the current crate version.
pub fn write_state(vault: &Vault, now_unix_secs: u64) -> std::io::Result<PathBuf> {
    let sidecar = ensure_sidecar_dir(vault.root())?;
    let path = sidecar.join("state.json");
    let state = VaultState {
        schema_version: STATE_SCHEMA_VERSION,
        vault_name: vault.name(),
        vault_root: vault.root().to_path_buf(),
        last_opened_at: now_unix_secs,
        opened_by: env!("CARGO_PKG_VERSION").to_string(),
    };
    let json = serde_json::to_string_pretty(&state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// Read `.tungsten/state.json` for a vault. Returns `None` if the
/// file doesn't exist; an error if the file exists but can't be
/// parsed.
pub fn read_state(vault_root: &Path) -> std::io::Result<Option<VaultState>> {
    let path = vault_root.join(".tungsten").join("state.json");
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)?;
    let state: VaultState = serde_json::from_slice(&bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(Some(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_vault(label: &str) -> Vault {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("tungsten-sidecar-{label}-{pid}-{n}"));
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir(dir.join(".obsidian")).unwrap();
        Vault::open(&dir).expect("vault")
    }

    #[test]
    fn ensure_sidecar_dir_creates_dir() {
        let vault = unique_vault("ensure");
        let sidecar = ensure_sidecar_dir(vault.root()).unwrap();
        assert!(sidecar.is_dir());
        assert_eq!(sidecar, vault.root().join(".tungsten"));
        fs::remove_dir_all(vault.root()).ok();
    }

    #[test]
    fn ensure_sidecar_dir_is_idempotent() {
        let vault = unique_vault("idem");
        ensure_sidecar_dir(vault.root()).unwrap();
        let again = ensure_sidecar_dir(vault.root()).unwrap();
        assert!(again.is_dir());
        fs::remove_dir_all(vault.root()).ok();
    }

    #[test]
    fn write_state_creates_file() {
        let vault = unique_vault("write");
        let path = write_state(&vault, 1_700_000_000).unwrap();
        assert!(path.is_file());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("schema_version"));
        assert!(content.contains(&vault.name()));
        assert!(content.contains("1700000000"));
        fs::remove_dir_all(vault.root()).ok();
    }

    #[test]
    fn read_state_round_trip() {
        let vault = unique_vault("roundtrip");
        write_state(&vault, 1_700_000_000).unwrap();
        let loaded = read_state(vault.root()).unwrap().expect("state exists");
        assert_eq!(loaded.vault_name, vault.name());
        assert_eq!(loaded.last_opened_at, 1_700_000_000);
        assert_eq!(loaded.schema_version, STATE_SCHEMA_VERSION);
        fs::remove_dir_all(vault.root()).ok();
    }

    #[test]
    fn read_state_missing_returns_none() {
        let vault = unique_vault("missing");
        let loaded = read_state(vault.root()).unwrap();
        assert!(loaded.is_none());
        fs::remove_dir_all(vault.root()).ok();
    }

    #[test]
    fn state_serde_is_stable() {
        let state = VaultState {
            schema_version: 1,
            vault_name: "Notes".into(),
            vault_root: PathBuf::from("/home/user/Notes"),
            last_opened_at: 1700000000,
            opened_by: "0.1.0".into(),
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: VaultState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, back);
    }
}
