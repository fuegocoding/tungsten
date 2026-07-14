//! `tungsten-find-broken` — find unresolved wikilinks and markdown
//! links in a vault.
//!
//! Usage:
//!     tungsten-find-broken <VAULT_PATH>
//!         Walk every .md file in the vault and report links that
//!         don't resolve to a real note by title or filename.
//!         Each broken link is printed as
//!             path:line:column: -> TARGET
//!
//! "Resolve" here means: the link's normalized target matches
//! either another note's H1 OR its filename basename. The
//! normalized form strips .md, leading ./, and path components,
//! and is lower-cased — same as the index's lookup.
//!
//! A target that points at a section or block ([[Note#Heading]]
//! or [[Note#^block]]) is still considered resolved if `Note`
//! exists; the heading/block target is a refinement of the note
//! and is reported only in the `-> TARGET` column.
//!
//! Exit codes:
//!     0  at least one broken link found
//!     1  no broken links
//!     2  bad arguments or index error
//!
//! Examples:
//!     tungsten-find-broken ~/Notes
//!     tungsten-find-broken ~/Notes | wc -l

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::NoteIndex;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-find-broken <VAULT_PATH>\n\
             \n\
             Walk every .md file in the vault and report links that\n\
             don't resolve to a real note. Each line is:\n\
             \n    path:line:column: -> TARGET"
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
    let mut found = 0;
    for note in index.notes() {
        for link in &note.links {
            // The link's target is the bare note name (already
            // stripped of #section / |alias by the parser).
            // A note is considered present if either by_title
            // (H1 or filename, case-insensitive) returns it.
            if index.by_title(&link.target).is_some() {
                continue;
            }
            // No match — this is a broken link.
            found += 1;
            println!("{}: -> {}", note.path.display(), link.target);
        }
    }
    if found == 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
