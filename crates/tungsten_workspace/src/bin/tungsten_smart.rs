//! `tungsten-smart` — list and evaluate smart folders for
//! a vault.
//!
//! Usage:
//!     tungsten-smart <VAULT_PATH>
//!
//! For each `.tungsten/smart/*.md` or `.obsidian/smart/*.md`
//! file, prints:
//!     title<TAB>row_count<TAB>path
//!
//! When the query fails to parse or execute, the row_count
//! is `-1` and the error is shown after the row.

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{evaluate_all, NoteIndex};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-smart <VAULT_PATH>\n\
             \n\
             List and evaluate smart folders. One line per\n\
             folder: title<TAB>row_count<TAB>path."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(2);
        }
    };
    let results = evaluate_all(&vault, &index);
    if results.is_empty() {
        eprintln!("(no smart folders; create one at .tungsten/smart/*.md)");
        return ExitCode::SUCCESS;
    }
    for (sf, result) in &results {
        match result {
            Ok(r) => println!("{}\t{}\t{}", sf.title, r.rows.len(), sf.path.display()),
            Err(e) => {
                println!("{}\t-1\t{}\terror: {}", sf.title, sf.path.display(), e);
            }
        }
    }
    ExitCode::SUCCESS
}
