//! `tungsten-sync` — encrypt a sync log into a folder.
//!
//! Usage:
//!     tungsten-sync <VAULT_PATH> <SYNC_FOLDER> [--passphrase=P]
//!
//! For each note in the vault, emits an `Add` op to a
//! local `SyncLog`, then writes every op as an encrypted
//! `.op` file in `<SYNC_FOLDER>/ops/`, plus a
//! `manifest.json` at the root. The passphrase is prompted
//! for if not supplied.
//!
//! The output is ready to be uploaded to any untrusted
//! transport (S3, WebDAV, Syncthing). Only the encrypted
//! envelopes leave the device.
//!
//! Exit codes:
//!     0  log written
//!     2  bad arguments or write error

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{write_log, Op, SyncKey, SyncLog};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-sync <VAULT_PATH> <SYNC_FOLDER> [--passphrase=P]\n\
             \n\
             Encrypt the vault's current state into a sync\n\
             folder. Each note becomes an Add op; the log is\n\
             written as encrypted envelopes under ops/ plus\n\
             a manifest.json at the root."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    let folder = PathBuf::from(&args[2]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let passphrase: String = args
        .iter()
        .find_map(|a| a.strip_prefix("--passphrase="))
        .map(String::from)
        .unwrap_or_else(|| prompt_passphrase());

    let key = match derive_key_interactive(&passphrase) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("derive_key: {e}");
            return ExitCode::from(2);
        }
    };

    // Build a sync log from the vault's current state.
    let index = match tungsten_workspace::NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(2);
        }
    };
    let mut local = SyncLog::new();
    for note in index.notes() {
        let mtime = note
            .mtime
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| {
                chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0)
                    .unwrap_or_else(chrono::Utc::now)
            })
            .unwrap_or_else(chrono::Utc::now);
        local.append(Op::Add {
            path: note.path.clone(),
            body: note.content.clone(),
            mtime,
        });
    }

    // For a first sync, the local log IS the source of
    // truth; push every op to the remote folder.
    match write_log(&local, &folder, &key) {
        Ok(paths) => {
            println!("wrote {} envelopes to {}", paths.len(), folder.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("write error: {e}");
            ExitCode::from(2)
        }
    }
}

fn prompt_passphrase() -> String {
    eprint!("passphrase: ");
    let mut s = String::new();
    if std::io::stdin().read_line(&mut s).is_err() {
        return String::new();
    }
    s.trim_end_matches(['\n', '\r']).to_string()
}

fn derive_key_interactive(passphrase: &str) -> Result<SyncKey, String> {
    use rand::RngCore;
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    tungsten_workspace::eaar::derive_key(passphrase.as_bytes(), &salt)
        .map_err(|e| e.to_string())
}
