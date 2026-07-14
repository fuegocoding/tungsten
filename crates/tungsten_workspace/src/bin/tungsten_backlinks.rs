//! `tungsten-backlinks` — list the notes that link to a given note.
//!
//! Usage:
//!     tungsten-backlinks <VAULT_PATH> <TARGET>
//!         Print every note in <VAULT_PATH> that has a wikilink
//!         or markdown link to <TARGET>. <TARGET> can be either a
//!         filename (e.g. \"MyNote.md\") or a title (e.g. \"My Note\").
//!         Matching is case-insensitive on both sides.
//!
//!     tungsten-backlinks <VAULT_PATH> --all-orphans
//!         Print every orphan note in the vault (notes with no
//!         incoming and no outgoing resolved links).
//!
//! Exit codes:
//!     0  success (even if there are zero results)
//!     1  index error or not a vault
//!     2  bad arguments

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::NoteIndex;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!(
            "usage: tungsten-backlinks <VAULT_PATH> <TARGET>\n\
             \n\
             Print every note in the vault that links to TARGET.\n\
             TARGET can be a filename (e.g. 'MyNote.md') or a title\n\
             (e.g. 'My Note'). Matching is case-insensitive."
        );
        return ExitCode::from(2);
    }
    let positional: Vec<&String> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    let vault = match positional.first() {
        Some(v) => PathBuf::from(v),
        None => {
            eprintln!("usage: tungsten-backlinks <VAULT_PATH> <TARGET>");
            return ExitCode::from(2);
        }
    };
    let target = match positional.get(1) {
        Some(t) => t.as_str(),
        None => {
            eprintln!("usage: tungsten-backlinks <VAULT_PATH> <TARGET>");
            return ExitCode::from(2);
        }
    };
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(1);
    }
    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(1);
        }
    };
    // The index normalizes link keys by stripping .md, leading ./,
    // and path components, then lowercasing. We do the same on
    // the target so the user can pass either a filename or a
    // title and the lookup will match.
    let lookup_target = normalize_target(target);
    let backlinks: Vec<_> = index.backlinks(&lookup_target).collect();
    println!("Backlinks for {} (normalized: {}):", target, lookup_target);
    println!("  {} match(es)", backlinks.len());
    for note in &backlinks {
        println!("  - {}", note.path.display());
    }
    ExitCode::SUCCESS
}

/// Mirror of NoteIndex::link_key's normalization but exported here
/// so the CLI can pre-normalize the user's input.
fn normalize_target(target: &str) -> String {
    let mut t = target.trim().to_string();
    if let Some(stripped) = t.strip_suffix(".md") {
        t = stripped.to_string();
    }
    if let Some(stripped) = t.strip_prefix("./") {
        t = stripped.to_string();
    }
    if let Some(idx) = t.rfind(['/', '\\']) {
        t = t[idx + 1..].to_string();
    }
    t.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_md_and_path() {
        assert_eq!(normalize_target("Note"), "note");
        assert_eq!(normalize_target("Note.md"), "note");
        assert_eq!(normalize_target("./Note"), "note");
        assert_eq!(normalize_target("folder/Note"), "note");
        assert_eq!(normalize_target("Note with spaces"), "note with spaces");
    }
}
