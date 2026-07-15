//! `tungsten-tasks` — aggregate open task markers across a
//! vault.
//!
//! Usage:
//!     tungsten-tasks <VAULT_PATH> [--done] [--by-note]
//!
//! Scans every note for `- [ ]` and `- [x]` (and `[X]`)
//! markers and prints a summary. By default only open
//! tasks are shown. With `--done`, completed tasks are
//! also printed. With `--by-note`, the output is grouped
//! by source note.
//!
//! Output format (default):
//!     <done?><TAB>path<TAB>line<TAB>task text
//!
//! Examples:
//!     tungsten-tasks ~/Notes
//!     tungsten-tasks ~/Notes --done | wc -l

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::NoteIndex;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-tasks <VAULT_PATH> [--done] [--by-note]\n\
             \n\
             Aggregate open task markers across a vault."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let show_done = args.iter().any(|a| a == "--done");
    let by_note = args.iter().any(|a| a == "--by-note");

    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(2);
        }
    };

    let mut open = 0;
    let mut done = 0;
    for note in index.notes() {
        for (i, line) in note.content.lines().enumerate() {
            let trimmed = line.trim_start();
            let after_dash = trimmed.strip_prefix("- ").unwrap_or(trimmed);
            if let Some(rest) = after_dash.strip_prefix("[ ] ") {
                if by_note {
                    println!("{}\t{}:{}", note.path.display(), i + 1, rest);
                } else {
                    println!(
                        " \t{}\t{}\t{}",
                        note.path.display(),
                        i + 1,
                        rest
                    );
                }
                open += 1;
            } else if let Some(rest) = after_dash
                .strip_prefix("[x] ")
                .or_else(|| after_dash.strip_prefix("[X] "))
            {
                done += 1;
                if show_done {
                    if by_note {
                        println!("{}\t{}:{}", note.path.display(), i + 1, rest);
                    } else {
                        println!(
                            "x\t{}\t{}\t{}",
                            note.path.display(),
                            i + 1,
                            rest
                        );
                    }
                }
            }
        }
    }
    eprintln!("{open} open, {done} done");
    ExitCode::SUCCESS
}
