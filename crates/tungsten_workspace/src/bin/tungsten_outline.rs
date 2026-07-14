//! `tungsten-outline` — print heading outlines for a vault or
//! a single file.
//!
//! Usage:
//!     tungsten-outline <VAULT_OR_FILE>
//!
//! For each note, prints:
//!     <note-path>\t<heading-text>
//!     <note-path>\t<heading-text>
//!     ...
//!
//! Headings are sorted in document order; the indentation
//! level is not preserved (it would require a more complex
//! output format). The first H1 of each note is the title
//! reference; deeper headings follow in order.
//!
//! When the argument is a directory it's treated as a vault.
//! When it's a single file, only that file is outlined.
//!
//! Examples:
//!     tungsten-outline ~/Notes | head
//!     tungsten-outline ~/Notes/welcome.md

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tungsten_workspace::NoteIndex;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-outline <VAULT_OR_FILE>\n\
             \n\
             Print heading outlines, one line per heading:\n  \
                 <note-path><TAB><heading-text>"
        );
        return ExitCode::from(2);
    }
    let target = PathBuf::from(&args[1]);
    if !target.exists() {
        eprintln!("not found: {}", target.display());
        return ExitCode::from(2);
    }

    if target.is_file() {
        let s = std::fs::read_to_string(&target).unwrap_or_default();
        print_outline(&target, &s);
    } else if target.is_dir() {
        let index = match NoteIndex::build(&target) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("index error: {e}");
                return ExitCode::from(2);
            }
        };
        for note in index.notes() {
            print_outline(&note.path, &note.content);
        }
    } else {
        eprintln!("not a file or directory: {}", target.display());
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

fn print_outline(path: &Path, content: &str) {
    for h in tungsten_workspace::outline(content) {
        for inner in h.flatten() {
            println!("{}\t{}", path.display(), inner.text);
        }
    }
}
