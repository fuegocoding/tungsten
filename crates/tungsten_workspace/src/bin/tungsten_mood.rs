//! `tungsten-mood` — quick mood logger.
//!
//! Usage:
//!     tungsten-mood <VAULT_PATH> <MOOD> [ENERGY]
//!
//! Appends `mood: <N>` and optionally `energy: <N>` to
//! today's daily note (creates it if missing). The values
//! must be 0..=10.
//!
//! Examples:
//!     tungsten-mood ~/Notes 7
//!     tungsten-mood ~/Notes 8 6
//!     tungsten-mood ~/Notes 5 4 --comment="Tough day"

use std::path::PathBuf;
use std::process::ExitCode;

use chrono::Local;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-mood <VAULT_PATH> <MOOD> [ENERGY] [--comment=TEXT]\n\
             \n\
             Append today's mood to the daily note. Both\n\
             values must be 0..=10."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let mood: u8 = match args[2].parse() {
        Ok(n) if n <= 10 => n,
        _ => {
            eprintln!("mood must be 0..=10, got {:?}", args[2]);
            return ExitCode::from(2);
        }
    };
    let energy: Option<u8> = args.get(3).and_then(|s| {
        s.parse::<u8>().ok().and_then(|n| if n <= 10 { Some(n) } else { None })
    });
    let comment: Option<String> = args
        .iter()
        .find_map(|a| a.strip_prefix("--comment=").map(String::from));

    let today = Local::now().date_naive();
    let folder = vault.join("Journal");
    if !folder.exists() {
        if let Err(e) = std::fs::create_dir_all(&folder) {
            eprintln!("mkdir error: {e}");
            return ExitCode::from(2);
        }
    }
    let path = folder.join(format!("{}.md", today.format("%Y-%m-%d")));

    let mut content = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        format!("# {}\n\n## Mood\n\nmood: \nenergy: \n", today.format("%Y-%m-%d"))
    });
    content = append_mood(&content, mood, energy, comment.as_deref());
    if let Err(e) = std::fs::write(&path, &content) {
        eprintln!("write error: {e}");
        return ExitCode::from(2);
    }
    println!(
        "logged mood={} energy={} in {}",
        mood,
        energy.map(|n| n.to_string()).unwrap_or_else(|| "-".into()),
        path.display()
    );
    ExitCode::SUCCESS
}

fn append_mood(content: &str, mood: u8, energy: Option<u8>, comment: Option<&str>) -> String {
    let mut out = content.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&format!("mood: {mood}\n"));
    if let Some(e) = energy {
        out.push_str(&format!("energy: {e}\n"));
    }
    if let Some(c) = comment {
        out.push_str(&format!("comment: {c}\n"));
    }
    out
}
