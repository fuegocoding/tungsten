//! `tungsten-init` — bootstrap a new vault at PATH.
//!
//! Usage:
//!     tungsten-init <PATH>
//!         Create PATH/.obsidian/, PATH/.tungsten/, an initial
//!         .tungsten/state.json, and a Welcome.md starter note.
//!         Idempotent: existing files are not overwritten.
//!
//! Exit codes:
//!     0  success
//!     1  path is not a directory
//!     2  bad arguments
//!
//! Errors are printed to stderr. On success, the absolute paths of
//! the created resources are printed to stdout.

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::TungstenWorkspace;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!("usage: tungsten-init <PATH>");
        return ExitCode::from(2);
    }
    let path = PathBuf::from(&args[1]);
    if !path.is_dir() {
        eprintln!("not a directory: {}", path.display());
        return ExitCode::from(1);
    }
    let now_unix_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut ws = TungstenWorkspace::new();
    let vault = match ws.init_vault(&path, now_unix_secs) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("init failed: {e}");
            return ExitCode::from(1);
        }
    };
    println!("Vault:      {}", vault.name());
    println!("Root:       {}", vault.root().display());
    println!("Config dir: {}", vault.config_dir().display());
    println!("Welcome:    {}", vault.root().join("Welcome.md").display());
    println!(
        "Sidecar:    {}",
        vault.root().join(".tungsten/state.json").display()
    );
    ExitCode::SUCCESS
}
