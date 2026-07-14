//! `tungsten-plugins` — list installed and enabled
//! community plugins for a vault.
//!
//! Usage:
//!     tungsten-plugins <VAULT_PATH>
//!
//! For each plugin under `.obsidian/plugins/<id>/`, prints:
//!     <enabled><TAB><id><TAB><name><TAB><version>
//!
//! `enabled` is `*` if the plugin is in
//! `community-plugins.json`, else a space.
//!
//! Examples:
//!     tungsten-plugins ~/Notes
//!     tungsten-plugins ~/Notes | awk -F'\t' '$1=="*{print $2}'

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{discover, parse_enabled_list};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-plugins <VAULT_PATH>\n\
             \n\
             List installed community plugins, one per line:\n  \
                 <enabled><TAB><id><TAB><name><TAB><version>"
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let obs_dir = vault.join(".obsidian");
    let enabled_ids = match parse_enabled_list(&obs_dir) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => {
            eprintln!("community-plugins.json: {e}");
            return ExitCode::from(2);
        }
    };
    let registry = discover(&obs_dir, &enabled_ids);
    for (_id, p) in registry.iter() {
        let enabled_marker = if p.enabled { "*" } else { " " };
        println!(
            "{}\t{}\t{}\t{}",
            enabled_marker, p.manifest.id, p.manifest.name, p.manifest.version
        );
    }
    ExitCode::SUCCESS
}
