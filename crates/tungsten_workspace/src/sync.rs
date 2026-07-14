//! Sync envelope and operation log (M7.x foundation).
//!
//! This module defines the on-disk format Tungsten uses
//! when syncing a vault to a remote folder. It is the
//! foundation; the actual transport (Etebase, Syncthing,
//! WebDAV, S3) is layered on top.
//!
//! # Threat model
//!
//! The remote is treated as untrusted. Every file payload is
//! encrypted with the vault's [`sync_key`] using
//! authenticated encryption (XChaCha20-Poly1305) before it
//! leaves the device. The transport only sees opaque
//! envelopes.
//!
//! # On-disk format
//!
//! A sync folder contains:
//!
//! - `manifest.json` — the latest cursor and a list of
//!   operations applied so far
//! - `ops/<op_id>.op` — one encrypted envelope per
//!   operation
//!
//! Each `op` file is a [`SyncEnvelope`] JSON blob with a
//! `nonce` and `ciphertext`. Decryption uses the vault's
//! [`sync_key`].
//!
//! # Operation types
//!
//! - [`Op::Add`] — a new file
//! - [`Op::Update`] — a new revision of an existing file
//! - [`Op::Delete`] — mark a path as removed
//! - [`Op::Move`] — rename a path
//!
//! The [`SyncLog`] is an append-only log of operations. The
//! [`reconcile`] function merges a remote log with a local
//! one, producing a list of operations the local side
//! should apply to catch up.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::eaar;

/// A sync key: 32 bytes derived from a passphrase via
/// Argon2id (or supplied directly). The same key encrypts
/// every envelope in a sync folder.
pub type SyncKey = [u8; 32];

/// Derive a sync key from a passphrase using Argon2id with
/// the Tungsten EaaR parameters.
///
/// A fresh random salt is generated for each call. Callers
/// that need a stable key (e.g. a previously-encrypted
/// folder) must persist the salt alongside the passphrase.
pub fn sync_key(passphrase: &str) -> Result<SyncKey, eaar::EaRError> {
    use rand::RngCore;
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let key = eaar::derive_key(passphrase.as_bytes(), &salt)?;
    Ok(key)
}

/// One operation in the sync log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Op {
    /// A new file path with a (possibly empty) body.
    Add { path: PathBuf, body: String, mtime: DateTime<Utc> },
    /// Replace the body of an existing path.
    Update { path: PathBuf, body: String, mtime: DateTime<Utc> },
    /// Mark a path as removed.
    Delete { path: PathBuf, mtime: DateTime<Utc> },
    /// Rename a path.
    Move { from: PathBuf, to: PathBuf, mtime: DateTime<Utc> },
}

impl Op {
    /// A short stable id derived from the operation. Used
    /// as the on-disk filename `<op_id>.op`.
    pub fn op_id(&self) -> String {
        let (path, kind) = match self {
            Op::Add { path, .. } => (path, "add"),
            Op::Update { path, .. } => (path, "update"),
            Op::Delete { path, .. } => (path, "delete"),
            Op::Move { from, .. } => (from, "move"),
        };
        format!("{}-{}", kind, path.display().to_string().replace('/', "_"))
    }

    /// Path affected by the operation. For `Move` this is
    /// the *source* path; the destination is in `to`.
    pub fn path(&self) -> &Path {
        match self {
            Op::Add { path, .. }
            | Op::Update { path, .. }
            | Op::Delete { path, .. } => path,
            Op::Move { from, .. } => from,
        }
    }

    /// Mtime of the operation.
    pub fn mtime(&self) -> DateTime<Utc> {
        match self {
            Op::Add { mtime, .. }
            | Op::Update { mtime, .. }
            | Op::Delete { mtime, .. }
            | Op::Move { mtime, .. } => *mtime,
        }
    }
}

/// The encrypted wire format for a single operation.
///
/// The payload is the EaaR file format (magic + version +
/// salt + nonce + ciphertext) hex-encoded. Decryption
/// goes through [`eaar::decrypt`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncEnvelope {
    /// Schema version, currently always `1`.
    pub version: u8,
    /// EaaR-format payload, hex-encoded.
    pub payload: String,
    /// The decrypted plaintext, populated after
    /// [`Self::decrypt`]. Skipped during serialization.
    #[serde(skip)]
    pub plaintext: Vec<u8>,
}

impl SyncEnvelope {
    /// Encrypt `op` under `key`, producing a self-contained
    /// envelope ready to write to disk.
    pub fn encrypt(op: &Op, key: &SyncKey) -> Result<Self, eaar::EaRError> {
        let plaintext = serde_json::to_vec(op).map_err(|e| {
            eaar::EaRError::Format(format!("encode op: {e}"))
        })?;
        let payload = eaar::encrypt(&plaintext, key)?;
        Ok(Self {
            version: 1,
            payload: hex_encode(&payload),
            plaintext,
        })
    }

    /// Decrypt the envelope and parse the operation.
    pub fn decrypt(&self, key: &SyncKey) -> Result<Op, eaar::EaRError> {
        let payload = hex_decode(&self.payload)
            .map_err(|e| eaar::EaRError::Format(format!("bad payload: {e}")))?;
        let plaintext = eaar::decrypt(&payload, key)?;
        let op: Op = serde_json::from_slice(&plaintext)
            .map_err(|e| eaar::EaRError::Format(format!("decode op: {e}")))?;
        Ok(op)
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("odd length".into());
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let hi = hex_nibble(bytes[i])?;
        let lo = hex_nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_nibble(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(format!("not a hex digit: {}", b as char)),
    }
}

/// A manifest file written to the sync folder root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncManifest {
    /// Last cursor: the highest op_id seen by this client.
    pub cursor: String,
    /// Total number of operations applied.
    pub op_count: usize,
    /// Mtime of the last write.
    pub last_modified: DateTime<Utc>,
}

impl Default for SyncManifest {
    fn default() -> Self {
        Self {
            cursor: String::new(),
            op_count: 0,
            last_modified: Utc::now(),
        }
    }
}

/// An append-only log of operations with a cursor for the
/// last applied position.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncLog {
    ops: Vec<Op>,
    /// Map from path to its latest known body. Used to
    /// short-circuit `Add` → `Update` sequences.
    bodies: BTreeMap<PathBuf, String>,
    /// Map from path to the latest mtime.
    mtimes: BTreeMap<PathBuf, DateTime<Utc>>,
    /// Tombstones (deleted paths).
    deleted: std::collections::BTreeSet<PathBuf>,
    /// Renames: source path → destination path.
    renames: Vec<(PathBuf, PathBuf)>,
}

impl SyncLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of operations in the log.
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Append an operation. The log mutates the
    /// path → body / mtime / tombstone indices so the
    /// caller can later ask "what does the latest state of
    /// this path look like?" without replaying.
    pub fn append(&mut self, op: Op) {
        match &op {
            Op::Add { path, body, mtime }
            | Op::Update { path, body, mtime } => {
                self.bodies.insert(path.clone(), body.clone());
                self.mtimes.insert(path.clone(), *mtime);
                self.deleted.remove(path);
            }
            Op::Delete { path, mtime } => {
                self.bodies.remove(path);
                self.mtimes.insert(path.clone(), *mtime);
                self.deleted.insert(path.clone());
            }
            Op::Move { from, to, mtime } => {
                if let Some(body) = self.bodies.remove(from) {
                    self.bodies.insert(to.clone(), body);
                }
                if let Some(mt) = self.mtimes.remove(from) {
                    self.mtimes.insert(to.clone(), mt);
                }
                self.deleted.remove(from);
                self.deleted.remove(to);
                self.renames.push((from.clone(), to.clone()));
                self.mtimes.insert(to.clone(), *mtime);
            }
        }
        self.ops.push(op);
    }

    /// Get the latest body for `path`, if any.
    pub fn latest_body(&self, path: &Path) -> Option<&str> {
        self.bodies.get(path).map(|s| s.as_str())
    }

    /// Get the latest mtime for `path`, if any.
    pub fn latest_mtime(&self, path: &Path) -> Option<DateTime<Utc>> {
        self.mtimes.get(path).copied()
    }

    /// True if the path is currently deleted.
    pub fn is_deleted(&self, path: &Path) -> bool {
        self.deleted.contains(path)
    }

    /// Iterate operations in append order.
    pub fn iter(&self) -> impl Iterator<Item = &Op> {
        self.ops.iter()
    }

    /// Build a manifest from the current state.
    pub fn manifest(&self) -> SyncManifest {
        SyncManifest {
            cursor: self
                .ops
                .last()
                .map(|op| op.op_id())
                .unwrap_or_default(),
            op_count: self.ops.len(),
            last_modified: self
                .ops
                .last()
                .map(|op| op.mtime())
                .unwrap_or_else(Utc::now),
        }
    }
}

/// Reconcile a remote log against a local one. The result
/// is a vector of operations the local side should apply
/// to catch up to the remote.
///
/// Conflict policy: when both sides modified the same path
/// at non-overlapping mtimes, the **later** mtime wins. The
/// losing operation is replaced by a `Noop` (which the log
/// discards) and the winning body is emitted as an
/// `Update`.
pub fn reconcile(remote: &SyncLog, local: &SyncLog) -> Vec<Op> {
    let mut out: Vec<Op> = Vec::new();
    for op in remote.iter() {
        match op {
            Op::Add { path, body, mtime } | Op::Update { path, body, mtime } => {
                match local.latest_mtime(path) {
                    None => out.push(op.clone()),
                    Some(local_mt) if local_mt < *mtime => {
                        // Remote is newer; rewrite as Update.
                        out.push(Op::Update {
                            path: path.clone(),
                            body: body.clone(),
                            mtime: *mtime,
                        });
                    }
                    Some(_) => {
                        // Local is at least as new. Skip.
                    }
                }
            }
            Op::Delete { path, mtime } => {
                if !local.is_deleted(path) {
                    out.push(Op::Delete {
                        path: path.clone(),
                        mtime: *mtime,
                    });
                }
            }
            Op::Move { from, to, mtime } => {
                if !local.is_deleted(from) {
                    out.push(Op::Move {
                        from: from.clone(),
                        to: to.clone(),
                        mtime: *mtime,
                    });
                }
            }
        }
    }
    out
}

/// Encode the log as a sequence of envelope files on disk.
/// Returns the absolute paths of the written files.
pub fn write_log(
    log: &SyncLog,
    folder: &Path,
    key: &SyncKey,
) -> std::io::Result<Vec<PathBuf>> {
    let ops_dir = folder.join("ops");
    std::fs::create_dir_all(&ops_dir)?;
    let mut paths = Vec::with_capacity(log.len());
    for op in log.iter() {
        let envelope = SyncEnvelope::encrypt(op, key)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let envelope_json = serde_json::to_vec_pretty(&envelope)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let path = ops_dir.join(format!("{}.op", op.op_id()));
        std::fs::write(&path, envelope_json)?;
        paths.push(path);
    }
    let manifest = log.manifest();
    let manifest_path = folder.join("manifest.json");
    let manifest_json = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    std::fs::write(manifest_path, manifest_json)?;
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(path: &str, body: &str) -> Op {
        Op::Add {
            path: PathBuf::from(path),
            body: body.to_string(),
            mtime: Utc::now(),
        }
    }

    fn update(path: &str, body: &str) -> Op {
        Op::Update {
            path: PathBuf::from(path),
            body: body.to_string(),
            mtime: Utc::now(),
        }
    }

    #[test]
    fn log_records_latest_body() {
        let mut log = SyncLog::new();
        log.append(add("A.md", "v1"));
        log.append(update("A.md", "v2"));
        assert_eq!(log.latest_body(Path::new("A.md")), Some("v2"));
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn delete_clears_body() {
        let mut log = SyncLog::new();
        log.append(add("A.md", "v1"));
        log.append(Op::Delete {
            path: PathBuf::from("A.md"),
            mtime: Utc::now(),
        });
        assert_eq!(log.latest_body(Path::new("A.md")), None);
        assert!(log.is_deleted(Path::new("A.md")));
    }

    #[test]
    fn reconcile_chooses_newer() {
        let older = Utc::now() - chrono::Duration::seconds(60);
        let newer = Utc::now();
        let mut remote = SyncLog::new();
        remote.append(Op::Update {
            path: PathBuf::from("A.md"),
            body: "remote-newer".into(),
            mtime: newer,
        });
        let mut local = SyncLog::new();
        local.append(Op::Add {
            path: PathBuf::from("A.md"),
            body: "local-older".into(),
            mtime: older,
        });
        let ops = reconcile(&remote, &local);
        assert_eq!(ops.len(), 1);
        if let Op::Update { body, .. } = &ops[0] {
            assert_eq!(body, "remote-newer");
        } else {
            panic!("expected Update");
        }
    }

    #[test]
    fn reconcile_skips_when_local_newer() {
        let older = Utc::now() - chrono::Duration::seconds(60);
        let newer = Utc::now();
        let mut remote = SyncLog::new();
        remote.append(Op::Add {
            path: PathBuf::from("A.md"),
            body: "remote-older".into(),
            mtime: older,
        });
        let mut local = SyncLog::new();
        local.append(Op::Add {
            path: PathBuf::from("A.md"),
            body: "local-newer".into(),
            mtime: newer,
        });
        let ops = reconcile(&remote, &local);
        assert!(ops.is_empty());
    }

    #[test]
    fn envelope_roundtrip() {
        let key = [0xABu8; 32];
        let op = add("A.md", "secret body");
        let env = SyncEnvelope::encrypt(&op, &key).unwrap();
        let restored = env.decrypt(&key).unwrap();
        assert_eq!(restored, op);
    }

    #[test]
    fn envelope_rejects_wrong_key() {
        let key = [0xABu8; 32];
        let bad = [0xCDu8; 32];
        let env = SyncEnvelope::encrypt(&add("A.md", "x"), &key).unwrap();
        assert!(env.decrypt(&bad).is_err());
    }

    #[test]
    fn op_id_stable_across_runs() {
        let op = add("notes/A.md", "x");
        let id1 = op.op_id();
        let id2 = op.op_id();
        assert_eq!(id1, id2);
    }

    #[test]
    fn manifest_tracks_count_and_cursor() {
        let mut log = SyncLog::new();
        log.append(add("A.md", "v1"));
        log.append(update("A.md", "v2"));
        log.append(add("B.md", "v1"));
        let m = log.manifest();
        assert_eq!(m.op_count, 3);
        assert!(!m.cursor.is_empty());
    }
}
