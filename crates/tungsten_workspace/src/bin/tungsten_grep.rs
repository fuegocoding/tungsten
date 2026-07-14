//! `tungsten-grep` — regex search across every .md file in a vault.
//!
//! Usage:
//!     tungsten-grep <VAULT_PATH> <PATTERN>
//!         Search every note's content for <PATTERN> (regex).
//!         Prints matches as `path:line:column: line`.
//!
//!     tungsten-grep <VAULT_PATH> <PATTERN> --no-filename
//!         Omit the path prefix; useful for piping to other tools.
//!
//!     tungsten-grep <VAULT_PATH> <PATTERN> -i
//!         Case-insensitive search.
//!
//! Exit codes:
//!     0  at least one match found
//!     1  no matches (or other search error)
//!     2  bad arguments
//!
//! Examples:
//!     tungsten-grep ~/Notes "TODO"
//!     tungsten-grep ~/Notes "fn .*\(" -i

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::NoteIndex;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!(
            "usage: tungsten-grep <VAULT_PATH> <PATTERN> [--no-filename] [-i]\n\
             \n\
             Search every note for PATTERN (regex). Prints matches as\n\
             'path:line:column: line'. Exit 0 if at least one match\n\
             was found, 1 otherwise."
        );
        return ExitCode::from(2);
    }
    let no_filename = args.iter().any(|a| a == "--no-filename");
    let case_insensitive = args.iter().any(|a| a == "-i");
    let positional: Vec<&String> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with("--") && *a != "-i")
        .collect();
    if positional.len() != 2 {
        eprintln!("usage: tungsten-grep <VAULT_PATH> <PATTERN> [--no-filename] [-i]");
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(positional[0]);
    let pattern_str = positional[1];

    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(1);
    }
    let re = match regex::RegexBuilder::new(pattern_str)
        .case_insensitive(case_insensitive)
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("invalid regex: {e}");
            return ExitCode::from(2);
        }
    };
    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(1);
        }
    };
    let mut found = 0;
    for note in index.notes() {
        for (i, line) in note.content.lines().enumerate() {
            for mat in re.find_iter(line) {
                found += 1;
                if no_filename {
                    println!("{}:{}", i + 1, mat.start() + 1);
                } else {
                    println!(
                        "{}:{}:{}: {}",
                        note.path.display(),
                        i + 1,
                        mat.start() + 1,
                        line
                    );
                }
            }
        }
    }
    if found == 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
