//! `tungsten-inspect` — print a diagnostic summary of a vault.
//!
//! Usage:
//!     tungsten-inspect <VAULT_PATH> [--json]
//!
//! Outputs (human-readable by default, JSON with `--json`):
//!   - Note, link, tag, orphan, broken-link counts
//!   - Attachment count (when computable)
//!   - Total size in bytes
//!   - Top 10 largest, newest, and oldest notes
//!   - Top 20 tags
//!   - Top 20 unresolved link targets
//!   - Folder distribution
//!
//! Exit codes:
//!     0  report produced
//!     2  bad arguments or index error

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{build_report, NoteIndex};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-inspect <VAULT_PATH> [--json]\n\
             \n\
             Print a diagnostic summary of the vault. With\n\
             --json the same data is emitted as a single JSON\n\
             object for piping into other tools."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let json = args.iter().any(|a| a == "--json");

    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(2);
        }
    };
    let attachment_count = count_attachments(&vault);
    let r = build_report(&index, attachment_count);

    if json {
        match serde_json::to_string_pretty(&r) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("json error: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        print_human(&r);
    }
    ExitCode::SUCCESS
}

fn count_attachments(vault: &std::path::Path) -> usize {
    let mut count = 0;
    for entry in walkdir::WalkDir::new(vault).into_iter().flatten() {
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                let ext = ext.to_ascii_lowercase();
                if matches!(
                    ext.as_str(),
                    "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "pdf" | "mp3" | "mp4" | "wav" | "mov"
                ) {
                    count += 1;
                }
            }
        }
    }
    count
}

fn print_human(r: &tungsten_workspace::InspectorReport) {
    println!("# Vault inspector");
    println!();
    println!("Notes:       {}", r.note_count);
    println!("Links:       {}", r.link_count);
    println!("Tags:        {}", r.tag_count);
    println!("Orphans:     {}", r.orphan_count);
    println!("Broken:      {}", r.broken_link_count);
    println!("Attachments: {}", r.attachment_count);
    println!("Total size:  {} bytes ({:.1} KB)", r.total_size_bytes, r.total_size_bytes as f64 / 1024.0);
    println!();
    if !r.largest_notes.is_empty() {
        println!("## Largest notes");
        for n in &r.largest_notes {
            println!("  {:>7} B  {}", n.size_bytes, n.path.display());
        }
        println!();
    }
    if !r.top_tags.is_empty() {
        println!("## Top tags");
        for t in &r.top_tags {
            println!("  {:>4}  #{}", t.count, t.tag);
        }
        println!();
    }
    if !r.top_broken_targets.is_empty() {
        println!("## Top broken targets");
        for t in &r.top_broken_targets {
            println!("  {:>4}  {}", t.count, t.target);
        }
        println!();
    }
    if !r.folder_distribution.is_empty() {
        println!("## Folder distribution");
        for f in &r.folder_distribution {
            println!("  {:>4}  {}", f.count, f.folder);
        }
    }
}
