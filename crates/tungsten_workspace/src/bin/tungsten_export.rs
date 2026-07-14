//! `tungsten-export` — export a vault to a single JSON
//! file.
//!
//! Usage:
//!     tungsten-export <VAULT_PATH> [--out=PATH]
//!
//! Emits a JSON document containing every note (path,
//! title, content, frontmatter, tags, links, callouts,
//! mtime) plus vault-wide statistics. With `--out=PATH`,
//! writes to the file; otherwise prints to stdout.

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{build_export, NoteIndex};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-export <VAULT_PATH> [--out=PATH]\n\
             \n\
             Export the vault to a single JSON document."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let out: Option<PathBuf> = args
        .iter()
        .find_map(|a| a.strip_prefix("--out=").map(PathBuf::from));

    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(2);
        }
    };
    let export = build_export(&index);
    let json = match serde_json::to_string_pretty(&export) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("json error: {e}");
            return ExitCode::from(2);
        }
    };
    match out {
        Some(p) => match std::fs::write(&p, &json) {
            Ok(_) => {
                eprintln!("wrote {}", p.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("write error: {e}");
                ExitCode::from(2)
            }
        },
        None => {
            print!("{json}");
            ExitCode::SUCCESS
        }
    }
}
