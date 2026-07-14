//! `tungsten-validate` — run pass/fail checks on a vault.
//!
//! Usage:
//!     tungsten-validate <VAULT_PATH> [--json] [--fail-on-warn]
//!
//! Runs six checks and prints a one-line summary per check
//! plus a final pass/fail count. Exit code is non-zero if
//! any check fails (CI-friendly).
//!
//! Examples:
//!     tungsten-validate ~/Notes
//!     tungsten-validate ~/Notes --json | jq '.failed'
//!     tungsten-validate ~/Notes && echo "vault healthy"

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{validate, NoteIndex};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-validate <VAULT_PATH> [--json]\n\
             \n\
             Run pass/fail checks; exit non-zero on any failure."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let json = args.iter().any(|a| a == "--json");

    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(2);
        }
    };
    let report = validate(&index);

    if json {
        match serde_json::to_string_pretty(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("json error: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        for c in &report.checks {
            let mark = if c.passed { "OK  " } else { "FAIL" };
            println!("[{}] {:<24} {}", mark, c.name, c.message);
        }
        println!();
        println!("{} passed, {} failed", report.passed, report.failed);
    }

    if report.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
