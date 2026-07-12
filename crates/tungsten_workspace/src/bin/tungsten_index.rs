//! `tungsten-index` — build a note index for a vault and print stats.
//!
//! Usage:
//!     tungsten-index <VAULT_PATH>
//!         Walks the vault, parses every .md file, and prints:
//!         - stats (note count, link count, tag count, orphan count)
//!         - the 20 most-common tags
//!         - the 20 most-linked-to notes (by backlinks count)
//!         - all orphans
//!
//! Exits 0 on success, 1 on index error, 2 on bad arguments.

use std::collections::BTreeMap;
use std::process::ExitCode;

use tungsten_workspace::NoteIndex;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!("usage: tungsten-index <VAULT_PATH>");
        return ExitCode::from(2);
    }
    let vault = std::path::PathBuf::from(&args[1]);
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

    let stats = index.stats();
    println!("stats:");
    println!("  notes:    {}", stats.note_count);
    println!("  links:    {}", stats.link_count);
    println!("  tags:     {}", stats.tag_count);
    println!("  orphans:  {}", stats.orphan_count);
    println!();

    // Top 20 tags.
    let mut tag_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for note in index.notes() {
        for tag in &note.tags {
            *tag_counts.entry(tag.as_str()).or_insert(0) += 1;
        }
    }
    let mut tag_vec: Vec<_> = tag_counts.iter().collect();
    tag_vec.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    println!("top tags ({}):", tag_vec.len().min(20));
    for (tag, count) in tag_vec.iter().take(20) {
        println!("  {count:>4}  #{tag}");
    }
    println!();

    // Top 20 most-linked-to notes.
    let mut backlink_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for n in index.notes() {
        let title = n.title.as_str();
        for bl in index.backlinks(title) {
            let t = bl.title.as_str();
            *backlink_counts.entry(t).or_insert(0) += 1;
        }
    }
    let mut bl_vec: Vec<_> = backlink_counts.iter().collect();
    bl_vec.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    println!("most-linked-to (top 20):");
    for (target, count) in bl_vec.iter().take(20) {
        println!("  {count:>4}  {target}");
    }
    println!();

    // Orphans.
    let orphans: Vec<&str> = index.orphans().iter().map(|n| n.title.as_str()).collect();
    println!("orphans ({}):", orphans.len());
    for o in &orphans {
        println!("  - {o}");
    }

    ExitCode::SUCCESS
}
