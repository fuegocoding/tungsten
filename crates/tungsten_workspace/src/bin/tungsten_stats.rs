//! `tungsten-stats` — word counts and reading time.
//!
//! Usage:
//!     tungsten-stats <VAULT_PATH> [--wpm=N] [--json]
//!
//! Prints:
//!     notes, words, characters, reading time
//!     top 10 longest notes by word count
//!     top 20 tags by word count
//!     word-length histogram (1-4, 5-8, 9-12, 13+)
//!
//! Default WPM is 220.
//!
//! Examples:
//!     tungsten-stats ~/Notes
//!     tungsten-stats ~/Notes --wpm=250 --json

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{compute_stats, NoteIndex};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-stats <VAULT_PATH> [--wpm=N] [--json]\n\
             \n\
             Compute word counts and reading time. Default\n\
             reading speed is 220 WPM."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let mut wpm: u32 = 220;
    let mut json = false;
    for a in &args[2..] {
        if let Some(rest) = a.strip_prefix("--wpm=") {
            if let Ok(n) = rest.parse() {
                wpm = n;
            }
        } else if a == "--json" {
            json = true;
        }
    }

    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(2);
        }
    };
    let s = compute_stats(&index, wpm);
    if json {
        match serde_json::to_string_pretty(&s) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("json error: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        println!("Notes:            {}", s.total_notes);
        println!("Words:            {}", s.total_words);
        println!("Characters:       {}", s.total_chars);
        println!("No whitespace:    {}", s.total_chars_no_whitespace);
        println!("Reading time:     {} min @ {} WPM", s.reading_time_minutes, wpm);
        println!();
        if !s.longest_notes.is_empty() {
            println!("## Longest notes");
            for n in &s.longest_notes {
                println!("  {:>6} words  {}", n.words, n.path.display());
            }
            println!();
        }
        if !s.top_tags.is_empty() {
            println!("## Top tags (by words)");
            for t in &s.top_tags {
                println!("  {:>5} words / {:>3} notes  #{}",
                    t.words, t.notes, t.tag);
            }
            println!();
        }
        println!("## Word length");
        println!("  1-4:   {}", s.word_histogram.short);
        println!("  5-8:   {}", s.word_histogram.medium);
        println!("  9-12:  {}", s.word_histogram.long);
        println!("  13+:   {}", s.word_histogram.extra_long);
    }
    ExitCode::SUCCESS
}
