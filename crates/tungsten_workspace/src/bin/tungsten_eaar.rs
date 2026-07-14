//! `tungsten-encrypt` / `tungsten-decrypt` — shell-side file
//! encryption using Tungsten's EaR layer.
//!
//! Usage:
//!     tungsten-encrypt <PASSPHRASE> <INPUT> <OUTPUT>
//!         Encrypt INPUT to OUTPUT using Argon2id +
//!         XChaCha20-Poly1305. The passphrase is taken from the
//!         first positional argument. Use single quotes around
//!         passphrases with spaces; shells will pass them
//!         through unchanged.
//!
//!     tungsten-decrypt <PASSPHRASE> <INPUT> <OUTPUT>
//!         Decrypt INPUT (a Tungsten EaR file) to OUTPUT.
//!
//! Exit codes:
//!     0  success
//!     1  encryption/decryption error
//!     2  bad arguments

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{decrypt_file, encrypt_file, EaRError};

fn run(mode: Mode, args: Vec<String>) -> ExitCode {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!(
            "usage: tungsten-{mode} <PASSPHRASE> <INPUT> <OUTPUT>\n\
             \n\
             Encrypt or decrypt a single file with the Tungsten EaR\n\
             layer (Argon2id + XChaCha20-Poly1305). The passphrase is\n\
             the first positional argument."
        );
        return ExitCode::from(2);
    }
    if args.len() != 4 {
        eprintln!(
            "usage: tungsten-{mode} <PASSPHRASE> <INPUT> <OUTPUT>"
        );
        return ExitCode::from(2);
    }
    let passphrase = args[1].as_bytes();
    let input = PathBuf::from(&args[2]);
    let output = PathBuf::from(&args[3]);
    if !input.is_file() {
        eprintln!("input not a file: {}", input.display());
        return ExitCode::from(1);
    }
    let result = match mode {
        Mode::Encrypt => encrypt_file(&input, &output, passphrase),
        Mode::Decrypt => decrypt_file(&input, &output, passphrase),
    };
    if let Err(e) = result {
        eprintln!("{mode} error: {e}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

#[derive(Clone, Copy)]
enum Mode {
    Encrypt,
    Decrypt,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Encrypt => f.write_str("encrypt"),
            Mode::Decrypt => f.write_str("decrypt"),
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let bin_name = std::path::Path::new(&args[0])
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let mode = if bin_name.contains("decrypt") {
        Mode::Decrypt
    } else if bin_name.contains("encrypt") {
        Mode::Encrypt
    } else {
        eprintln!("binary name must contain 'encrypt' or 'decrypt'");
        return ExitCode::from(2);
    };
    run(mode, args)
}
