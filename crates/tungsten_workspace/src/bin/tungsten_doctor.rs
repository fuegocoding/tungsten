//! `tungsten-doctor` — aggregate health report for a vault.
//!
//! Runs every health-oriented tool against the vault and
//! summarizes the result in a single human-readable
//! report. The tool is intended to be the first thing a
//! user runs when something feels off.
//!
//! Output sections:
//!     - Vault inspector summary (notes, links, tags, etc.)
//!     - Validate checks (with pass/fail per check)
//!     - Stats (words, reading time)
//!     - Plugin registry (installed + enabled)
//!     - Themes installed
//!
//! Exit code is non-zero if any validate check fails.
//!
//! Examples:
//!     tungsten-doctor ~/Notes
//!     tungsten-doctor ~/Notes --json | jq '.checks'

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{
    build_report, compute_stats, discover as discover_plugins, discover_templates,
    list_themes as discover_themes, parse_enabled_list, validate, NoteIndex,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-doctor <VAULT_PATH> [--json]\n\
             \n\
             Aggregate health report combining inspect,\n\
             validate, stats, and plugin/theme discovery."
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
    let inspector = build_report(&index, attachment_count);
    let report = validate(&index);
    let stats = compute_stats(&index, 220);
    let obs_dir = vault.join(".obsidian");
    let enabled_ids = parse_enabled_list(&obs_dir).unwrap_or_default();
    let plugin_registry = discover_plugins(&obs_dir, &enabled_ids);
    let themes = discover_themes(&vault).unwrap_or_default();
    let templates = discover_templates(&vault);

    if json {
        let out = serde_json::json!({
            "inspector": inspector,
            "validate": report,
            "stats": stats,
            "plugins": {
                "installed": plugin_registry.len(),
                "enabled": plugin_registry.enabled().count(),
            },
            "themes": themes.len(),
            "templates": templates.len(),
        });
        match serde_json::to_string_pretty(&out) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("json error: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        println!("# Tungsten doctor");
        println!();
        println!("## Vault");
        println!("  notes:      {}", inspector.note_count);
        println!("  links:      {}", inspector.link_count);
        println!("  tags:       {}", inspector.tag_count);
        println!("  orphans:    {}", inspector.orphan_count);
        println!("  broken:     {}", inspector.broken_link_count);
        println!();
        println!("## Checks");
        for c in &report.checks {
            let mark = if c.passed { "OK  " } else { "FAIL" };
            println!("  [{}] {:<24} {}", mark, c.name, c.message);
        }
        println!();
        println!("## Stats");
        println!("  words:    {}", stats.total_words);
        println!("  reading:  {} min", stats.reading_time_minutes);
        println!();
        println!("## Plugins");
        println!(
            "  installed: {}, enabled: {}",
            plugin_registry.len(),
            plugin_registry.enabled().count()
        );
        println!();
        println!("## Themes");
        println!("  installed: {}", themes.len());
        println!();
        println!("## Templates");
        println!("  installed: {}", templates.len());
    }
    if report.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
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
