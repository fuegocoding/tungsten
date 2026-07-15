//! `tungsten-search` — regex search with snippets.
//!
//! Like `tungsten-grep` but each match is followed by a
//! short highlighted snippet centered on the match.
//! Useful for an interactive search results panel.
//!
//! Usage:
//!     tungsten-search <VAULT_PATH> <PATTERN> [--max=120]
//!
//! Output for every match:
//!     <path>:<line>:<col>:<text up to match>**<match>**<after>
//!     ...
//!     --
//!     <path>:<line>:<col>:<next match...>
//!     ...
//!
//! Multiple matches in the same note are grouped under one
//! path; notes are separated by `--`.
//!
//! Examples:
//!     tungsten-search ~/Notes "TODO" | head
//!     tungsten-search ~/Notes "Argon2id" --max=80

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{snippet, NoteIndex};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-search <VAULT_PATH> <PATTERN> [--max=N]\n\
             \n\
             Regex search with snippets. Each match shows\n\
             path:line:col and a highlighted snippet."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let pattern = &args[2];
    let mut max_chars: usize = 120;
    for a in &args[3..] {
        if let Some(rest) = a.strip_prefix("--max=") {
            if let Ok(n) = rest.parse() {
                max_chars = n;
            }
        }
    }
    let re = match regex::Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("regex error: {e}");
            return ExitCode::from(2);
        }
    };

    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(2);
        }
    };

    let mut total = 0;
    for note in index.notes() {
        let mut note_hits: Vec<(usize, usize, String)> = Vec::new();
        for (line_idx, line) in note.content.lines().enumerate() {
            if let Some(m) = re.find(line) {
                let s = snippet(line, m.start()..m.end(), max_chars);
                note_hits.push((line_idx + 1, m.start() + 1, s));
            }
        }
        if !note_hits.is_empty() {
            if total > 0 {
                println!("--");
            }
            for (line, col, snip) in &note_hits {
                println!("{}:{}:{}:{}", note.path.display(), line, col, snip);
                total += 1;
            }
        }
    }
    eprintln!("{total} match(es)");
    if total == 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
