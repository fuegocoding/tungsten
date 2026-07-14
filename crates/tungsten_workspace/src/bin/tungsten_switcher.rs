//! `tungsten-switcher` — quick switcher (cmd-O) for vault notes.
//!
//! Usage:
//!     tungsten-switcher <VAULT_PATH> [QUERY]
//!
//! Prints up to 20 ranked hits, one per line:
//!     score<TAB>match-kind<TAB>title<TAB>path
//!
//! The score is lower-is-better. Ties are broken by path. When
//! the query is omitted (or empty), recent notes are listed.
//!
//! Examples:
//!     tungsten-switcher ~/Notes
//!     tungsten-switcher ~/Notes welcome
//!     tungsten-switcher ~/Notes '#work' | head

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{quick_switch, NoteIndex};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-switcher <VAULT_PATH> [QUERY]\n\
             \n\
             Print ranked quick-switcher hits, one per line:\n  \
                 score<TAB>match-kind<TAB>title<TAB>path"
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let query = args.get(2).cloned().unwrap_or_default();

    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(2);
        }
    };
    let hits = quick_switch(&index, &query, &[], 20);
    for hit in &hits {
        let kind = match hit.match_kind {
            tungsten_workspace::MatchKind::ExactTitle => "exact",
            tungsten_workspace::MatchKind::TitlePrefix => "prefix",
            tungsten_workspace::MatchKind::TitleContains => "contains",
            tungsten_workspace::MatchKind::FilenameMatch => "filename",
            tungsten_workspace::MatchKind::TagMatch => "tag",
            tungsten_workspace::MatchKind::RecentOpen => "recent",
        };
        println!(
            "{}\t{}\t{}\t{}",
            hit.score,
            kind,
            hit.note.title,
            hit.note.path.display()
        );
    }
    ExitCode::SUCCESS
}
