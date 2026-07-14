//! `tungsten-themes` — list installed themes and print the
//! base Tungsten theme's variables.
//!
//! Usage:
//!     tungsten-themes <VAULT_PATH> [--base] [--json]
//!
//! By default, lists every theme installed under
//! `.obsidian/themes/`. With `--base`, prints the variables
//! of Tungsten's built-in default theme instead. With
//! `--json`, emits JSON.
//!
//! Examples:
//!     tungsten-themes ~/Notes
//!     tungsten-themes ~/Notes --base --json | head

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{base_theme, list_themes};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-themes <VAULT_PATH> [--base] [--json]\n\
             \n\
             List installed themes (default), or print the\n\
             built-in base theme with --base."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let show_base = args.iter().any(|a| a == "--base");
    let json = args.iter().any(|a| a == "--json");

    if show_base {
        let t = base_theme();
        if json {
            match serde_json::to_string_pretty(&t) {
                Ok(s) => println!("{s}"),
                Err(e) => {
                    eprintln!("json error: {e}");
                    return ExitCode::from(2);
                }
            }
        } else {
            println!("{} ({})", t.name, t.author);
            if let Some(m) = t.mode {
                println!("  mode: {}", m.as_str());
            }
            for (k, v) in &t.variables {
                println!("  {k}: {v}");
            }
        }
    } else {
        let obs_dir = vault.join(".obsidian");
        let themes = match list_themes(&obs_dir) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("themes error: {e}");
                return ExitCode::from(2);
            }
        };
        if json {
            match serde_json::to_string_pretty(&themes) {
                Ok(s) => println!("{s}"),
                Err(e) => {
                    eprintln!("json error: {e}");
                    return ExitCode::from(2);
                }
            }
        } else {
            if themes.is_empty() {
                println!("(no themes installed)");
            } else {
                for t in &themes {
                    let mode = t
                        .mode
                        .map(|m| m.as_str())
                        .unwrap_or("?");
                    println!("{}\t{}\t{}", t.name, mode, t.author);
                }
            }
        }
    }
    ExitCode::SUCCESS
}
