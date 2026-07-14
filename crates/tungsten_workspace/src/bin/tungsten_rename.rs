//! `tungsten-rename` — rename a note in a vault and rewrite all
//! links that point to it.
//!
//! Usage:
//!     tungsten-rename <VAULT_ROOT> <OLD_REL> <NEW_REL>
//!         Rename <VAULT_ROOT>/<OLD_REL> to <VAULT_ROOT>/<NEW_REL>,
//!         rewriting every wikilink and markdown link in the rest
//!         of the vault that targets the old filename.
//!
//!     tungsten-rename --no-write <VAULT_ROOT> <OLD_REL> <NEW_REL>
//!         Same, but do a dry run: print what would change but
//!         don't actually rename or rewrite. Useful for previewing
//!         the impact of a rename.
//!
//! Examples:
//!     tungsten-rename ~/Notes OldNote.md NewNote.md
//!     tungsten-rename ~/Notes journal/2026-07-12.md journal/2026-07-13.md
//!     tungsten-rename --no-write ~/Notes todo.md done.md
//!
//! Exit codes:
//!     0  rename succeeded
//!     1  rename error
//!     2  bad arguments

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{NoteIndex, Vault};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!(
            "usage: tungsten-rename [--no-write] <VAULT_ROOT> <OLD_REL> <NEW_REL>\n\
             \n\
             Rename <VAULT_ROOT>/<OLD_REL> to <VAULT_ROOT>/<NEW_REL>,\n\
             rewriting every wikilink/markdown link in the vault that\n\
             pointed to the old filename."
        );
        return ExitCode::from(2);
    }
    let dry_run = args.iter().any(|a| a == "--no-write");
    let positional: Vec<&String> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    if positional.len() != 3 {
        eprintln!("usage: tungsten-rename [--no-write] <VAULT_ROOT> <OLD_REL> <NEW_REL>");
        return ExitCode::from(2);
    }
    let vault_root = PathBuf::from(positional[0]);
    let old_rel = PathBuf::from(positional[1]);
    let new_rel = PathBuf::from(positional[2]);

    let Some(vault) = Vault::open(&vault_root) else {
        eprintln!("not a vault: {}", vault_root.display());
        return ExitCode::from(1);
    };
    let old_abs = vault.root().join(&old_rel);
    let new_abs = vault.root().join(&new_rel);
    if !old_abs.is_file() {
        eprintln!("source file does not exist: {}", old_abs.display());
        return ExitCode::from(1);
    }
    if new_abs.exists() && old_abs != new_abs {
        eprintln!("destination already exists: {}", new_abs.display());
        return ExitCode::from(1);
    }

    // Build the index to discover what would change.
    let mut index = match NoteIndex::build(vault.root()) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(1);
        }
    };
    // We need a stable relative path for the old file; the
    // index keys by canonicalized path. We compute it via
    // stripping the vault root and joining.
    let old_canon = old_abs.canonicalize().unwrap_or_else(|_| old_abs.clone());
    let new_canon = new_abs.canonicalize().unwrap_or_else(|_| new_abs.clone());

    if dry_run {
        // Dry run: just report what backlinks point to the
        // old filename. Note: the index keys by H1, not by
        // filename basename (see rename.rs known limitations),
        // so this may show false positives for files whose H1
        // matches the old basename.
        let old_basename = old_abs
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled");
        let backlinks: Vec<_> = index.backlinks(old_basename).collect();
        println!("DRY RUN: would rename");
        println!("  {} -> {}", old_rel.display(), new_rel.display());
        println!("  old filename:     {old_basename}");
        println!("  backlinks:        {} note(s)", backlinks.len());
        for n in &backlinks {
            println!("    - {}", n.path.display());
        }
        ExitCode::SUCCESS
    } else {
        match index.rename(&old_canon, &new_canon) {
            Ok(result) => {
                println!("Renamed {} -> {}", old_rel.display(), new_rel.display());
                println!("  old link_key:   {}", result.old_title);
                println!("  new link_key:   {}", result.new_title);
                println!("  rewritten in:   {} source(s)", result.rewritten_sources.len());
                println!("  link count:     {}", result.link_replacements);
                for src in &result.rewritten_sources {
                    println!("    - {}", src.display());
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("rename error: {e}");
                ExitCode::from(1)
            }
        }
    }
}
