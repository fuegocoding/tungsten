//! `tungsten-diff` — show notes changed since a date.
//!
//! Usage:
//!     tungsten-diff <VAULT_PATH> [--since=YYYY-MM-DD] [--until=YYYY-MM-DD]
//!
//! Lists every note with an mtime in the given range. The
//! default `--since` is 7 days ago. `--until` defaults to
//! now. The output is one line per note:
//!
//!     mtime<TAB>size<TAB>path
//!
//! Examples:
//!     tungsten-diff ~/Notes
//!     tungsten-diff ~/Notes --since=2026-01-01
//!     tungsten-diff ~/Notes --since=2026-01-01 --until=2026-06-30

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, NaiveDate, Utc};

use tungsten_workspace::NoteIndex;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-diff <VAULT_PATH> [--since=DATE] [--until=DATE]\n\
             \n\
             List notes changed in a date range. Default\n\
             --since is 7 days ago; --until is now."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }

    let now: DateTime<Utc> = Utc::now();
    let default_since = now - chrono::Duration::days(7);
    let mut since: Option<DateTime<Utc>> = Some(default_since);
    let mut until: Option<DateTime<Utc>> = Some(now);
    for a in &args[2..] {
        if let Some(rest) = a.strip_prefix("--since=") {
            since = Some(parse_date(rest, default_since));
        } else if let Some(rest) = a.strip_prefix("--until=") {
            until = Some(parse_date(rest, now));
        }
    }

    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(2);
        }
    };

    let since_ts = since.unwrap_or(default_since);
    let until_ts = until.unwrap_or(now);
    let since_s = since_ts.timestamp() as u64;
    let until_s = until_ts.timestamp() as u64;

    let mut count = 0;
    for note in index.notes() {
        let Some(mtime) = note.mtime else { continue };
        let secs = mtime
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        if secs < since_s || secs > until_s {
            continue;
        }
        let dt: DateTime<Utc> = DateTime::<Utc>::from_timestamp(secs as i64, 0)
            .unwrap_or_else(|| Utc::now());
        println!(
            "{}\t{:>7}\t{}",
            dt.format("%Y-%m-%d %H:%M"),
            note.size_bytes,
            note.path.display()
        );
        count += 1;
    }
    eprintln!(
        "{} note(s) changed between {} and {}",
        count,
        since_ts.format("%Y-%m-%d"),
        until_ts.format("%Y-%m-%d")
    );
    ExitCode::SUCCESS
}

fn parse_date(s: &str, default: DateTime<Utc>) -> DateTime<Utc> {
    // Try a few common formats.
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return d
            .and_hms_opt(0, 0, 0)
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
            .unwrap_or(default);
    }
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return d.with_timezone(&Utc);
    }
    default
}
