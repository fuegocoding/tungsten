//! Encryption at Rest (EaR) — symmetric file encryption for
//! vault content.
//!
//! PRD §5.8: vaults can be stored as encrypted volumes using
//! Argon2id for KDF and XChaCha20-Poly1305 for AEAD. The full
//! design includes a FUSE/Dokany transparent-mount layer; this
//! module is the cryptographic core (key derivation, encryption,
//! decryption, file-format serialization). The transparent-mount
//! is a future commit (and is OS-specific: FUSE on macOS/Linux,
//! Dokany on Windows).
//!
//! **File format** (per-encrypted-file, the on-disk payload):
//!
//! ```text
//! magic   = b"tungstenEaR1"   (12 bytes, no null terminator)
//! version = u8                 (currently 1)
//! salt    = [u8; 16]           (per-file random salt for Argon2id)
//! nonce   = [u8; 24]           (XChaCha20-Poly1305 nonce)
//! ciphertext                       (XChaCha20-Poly1305 AEAD output,
//!                                   includes the 16-byte auth tag)
//! ```
//!
//! **Key derivation:** Argon2id with sensible defaults. The
//! salt is per-file so the same passphrase can encrypt any number
//! of files with distinct derived keys. The derived key is 32
//! bytes (XChaCha20-Poly1305's key size).
//!
//! **Per-file format** (versus per-vault): chosen for simplicity.
//! The FUSE-mount layer (M5.2) would migrate to a per-vault
//! wrapped-DEK pattern where a single KEK derives a per-file DEK.
//! For the library, per-file salting is fine and keeps the
//! surface area small.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand::RngCore;

const MAGIC: &[u8; 12] = b"tungstenEaR1";
const VERSION: u8 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;

/// Errors from the EaR layer.
#[derive(Debug, thiserror::Error)]
pub enum EaRError {
    #[error("argon2 error: {0}")]
    Argon2(String),
    #[error("AEAD seal/open error: {0}")]
    Aead(String),
    #[error("file format error: {0}")]
    Format(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Derive a 32-byte key from a passphrase + salt using Argon2id.
pub fn derive_key(passphrase: &[u8], salt: &[u8]) -> Result<[u8; KEY_LEN], EaRError> {
    let params = Params::new(19_456, 2, 1, Some(KEY_LEN))
        .map_err(|e| EaRError::Argon2(e.to_string()))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = [0u8; KEY_LEN];
    argon
        .hash_password_into(passphrase, salt, &mut out)
        .map_err(|e| EaRError::Argon2(e.to_string()))?;
    Ok(out)
}

/// Encrypt `plaintext` with `key` and return the on-disk file
/// payload. A fresh random salt and nonce are generated.
pub fn encrypt(plaintext: &[u8], key: &[u8; KEY_LEN]) -> Result<Vec<u8>, EaRError> {
    encrypt_with_salt(plaintext, key, &random_salt())
}

fn random_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
    salt
}

/// Internal: encrypt with an explicit salt (so the caller can
/// preserve the salt for use in key derivation).
fn encrypt_with_salt(
    plaintext: &[u8],
    key: &[u8; KEY_LEN],
    salt: &[u8; SALT_LEN],
) -> Result<Vec<u8>, EaRError> {
    let mut nonce = [0u8; NONCE_LEN];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    // Bind the salt to the ciphertext via the AEAD's associated
    // data. This means the salt can't be silently swapped (a
    // different salt wouldn't decrypt).
    let mut aad = Vec::with_capacity(MAGIC.len() + 1 + SALT_LEN + NONCE_LEN);
    aad.extend_from_slice(MAGIC);
    aad.push(VERSION);
    aad.extend_from_slice(salt);
    aad.extend_from_slice(&nonce);
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|e| EaRError::Aead(e.to_string()))?;
    let mut out = Vec::with_capacity(MAGIC.len() + 1 + SALT_LEN + NONCE_LEN + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.extend_from_slice(salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt an on-disk payload back to plaintext. Returns an
/// error if the key is wrong, the file is corrupt, or the format
/// version is unknown.
pub fn decrypt(payload: &[u8], key: &[u8; KEY_LEN]) -> Result<Vec<u8>, EaRError> {
    let header_len = MAGIC.len() + 1 + SALT_LEN + NONCE_LEN;
    if payload.len() < header_len {
        return Err(EaRError::Format("payload too short".into()));
    }
    if &payload[..MAGIC.len()] != MAGIC {
        return Err(EaRError::Format("bad magic".into()));
    }
    if payload[MAGIC.len()] != VERSION {
        return Err(EaRError::Format(format!(
            "unsupported version {}",
            payload[MAGIC.len()]
        )));
    }
    let salt: [u8; SALT_LEN] = payload[MAGIC.len() + 1..MAGIC.len() + 1 + SALT_LEN]
        .try_into()
        .map_err(|_| EaRError::Format("bad salt".into()))?;
    let nonce: [u8; NONCE_LEN] = payload[MAGIC.len() + 1 + SALT_LEN..header_len]
        .try_into()
        .map_err(|_| EaRError::Format("bad nonce".into()))?;
    let ciphertext = &payload[header_len..];
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    let mut aad = Vec::with_capacity(header_len);
    aad.extend_from_slice(&payload[..header_len]);
    cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: ciphertext,
                aad: &aad,
            },
        )
        .map_err(|e| EaRError::Aead(e.to_string()))
}

/// Convenience: encrypt a file on disk in place. Reads
/// `input_path`, writes encrypted bytes to `output_path`.
pub fn encrypt_file(
    input_path: &std::path::Path,
    output_path: &std::path::Path,
    passphrase: &[u8],
) -> Result<(), EaRError> {
    let plaintext = std::fs::read(input_path)?;
    let salt = random_salt();
    let key = derive_key(passphrase, &salt)?;
    let payload = encrypt_with_salt(&plaintext, &key, &salt)?;
    std::fs::write(output_path, payload)?;
    Ok(())
}

/// Convenience: decrypt a file on disk in place.
pub fn decrypt_file(
    input_path: &std::path::Path,
    output_path: &std::path::Path,
    passphrase: &[u8],
) -> Result<(), EaRError> {
    let payload = std::fs::read(input_path)?;
    // We don't need the salt from the file — derive_key takes
    // the salt, and we extract it from the file payload before
    // calling decrypt. The salt is at a known offset.
    let salt_start = MAGIC.len() + 1;
    let salt_end = salt_start + SALT_LEN;
    if payload.len() < salt_end {
        return Err(EaRError::Format("payload too short for salt".into()));
    }
    let salt: [u8; SALT_LEN] = payload[salt_start..salt_end]
        .try_into()
        .map_err(|_| EaRError::Format("bad salt".into()))?;
    let key = derive_key(passphrase, &salt)?;
    let plaintext = decrypt(&payload, &key)?;
    std::fs::write(output_path, plaintext)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn key() -> [u8; KEY_LEN] {
        let mut k = [0u8; KEY_LEN];
        for (i, b) in k.iter_mut().enumerate() {
            *b = i as u8;
        }
        k
    }

    #[test]
    fn round_trip_string() {
        let plaintext = b"hello, world";
        let k = key();
        let payload = encrypt(plaintext, &k).unwrap();
        let back = decrypt(&payload, &k).unwrap();
        assert_eq!(back, plaintext);
    }

    #[test]
    fn wrong_key_fails() {
        let plaintext = b"hello, world";
        let k1 = key();
        let mut k2 = key();
        k2[0] ^= 0xff;
        let payload = encrypt(plaintext, &k1).unwrap();
        let err = decrypt(&payload, &k2);
        assert!(err.is_err(), "wrong key should fail to decrypt");
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let plaintext = b"hello, world";
        let k = key();
        let mut payload = encrypt(plaintext, &k).unwrap();
        let last = payload.len() - 1;
        payload[last] ^= 0x01;
        let err = decrypt(&payload, &k);
        assert!(err.is_err(), "tampered ciphertext should fail");
    }

    #[test]
    fn bad_magic_fails() {
        let mut payload = vec![0u8; 256];
        payload[..12].copy_from_slice(b"NOTtungsten!");
        let err = decrypt(&payload, &key());
        assert!(err.is_err());
    }

    #[test]
    fn payload_layout_matches_documented_format() {
        let plaintext = b"x";
        let payload = encrypt(plaintext, &key()).unwrap();
        // magic (12) + version (1) + salt (16) + nonce (24) + ct
        assert_eq!(&payload[..12], MAGIC);
        assert_eq!(payload[12], VERSION);
        // rest is the AEAD output (which includes the 16-byte tag
        // for a 1-byte plaintext, so ct length is 1+16 = 17)
        assert_eq!(payload.len(), 12 + 1 + 16 + 24 + 1 + 16);
    }

    #[test]
    fn derive_key_is_deterministic() {
        let salt = [0xab; SALT_LEN];
        let k1 = derive_key(b"correct horse battery staple", &salt).unwrap();
        let k2 = derive_key(b"correct horse battery staple", &salt).unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn derive_key_differs_per_salt() {
        let passphrase = b"correct horse battery staple";
        let s1 = [0xab; SALT_LEN];
        let mut s2 = [0xab; SALT_LEN];
        s2[0] ^= 0x01;
        let k1 = derive_key(passphrase, &s1).unwrap();
        let k2 = derive_key(passphrase, &s2).unwrap();
        assert_ne!(k1, k2);
    }

    #[test]
    fn file_round_trip() {
        let dir = std::env::temp_dir().join(format!(
            "tungsten-ear-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let in_path = dir.join("plain.md");
        let enc_path = dir.join("plain.md.ear");
        let out_path = dir.join("decrypted.md");
        let mut f = std::fs::File::create(&in_path).unwrap();
        f.write_all(b"the rain in spain falls mainly on the plain")
            .unwrap();
        f.sync_all().unwrap();
        drop(f);

        let passphrase = b"hunter2";
        encrypt_file(&in_path, &enc_path, passphrase).unwrap();
        decrypt_file(&enc_path, &out_path, passphrase).unwrap();

        let round = std::fs::read(&out_path).unwrap();
        assert_eq!(round, b"the rain in spain falls mainly on the plain");

        std::fs::remove_dir_all(&dir).ok();
    }
}
