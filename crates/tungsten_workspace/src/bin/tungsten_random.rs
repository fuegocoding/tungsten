//! `tungsten-random` — open a random note from a vault.
//!
//! Usage:
//!     tungsten-random <VAULT_PATH> [--tag=TAG] [--count=N]
//!
//! With no `--tag`, picks uniformly from every note. With
//! `--tag`, picks from notes carrying that tag. With
//! `--count=N`, prints N paths (no duplicates). The output
//! is one path per line; the first path can be piped to
//! `xdg-open` (Linux), `open` (macOS), or `start` (Windows).
//!
//! Examples:
//!     tungsten-random ~/Notes
//!     tungsten-random ~/Notes --count=5
//!     tungsten-random ~/Notes --tag=writing
//!     tungsten-random ~/Notes | head -1 | xargs xdg-open

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::NoteIndex;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-random <VAULT_PATH> [--tag=T] [--count=N]\n\
             \n\
             Pick one or more random notes. With --count=N,\n\
             print N distinct paths."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let mut count: usize = 1;
    let mut tag: Option<String> = None;
    for a in &args[2..] {
        if let Some(rest) = a.strip_prefix("--count=") {
            if let Ok(n) = rest.parse() {
                count = n;
            }
        } else if let Some(rest) = a.strip_prefix("--tag=") {
            tag = Some(rest.to_string());
        }
    }

    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(2);
        }
    };
    let pool: Vec<PathBuf> = if let Some(t) = &tag {
        index
            .with_tag(t)
            .map(|n| n.path.clone())
            .collect()
    } else {
        index.notes().map(|n| n.path.clone()).collect()
    };
    if pool.is_empty() {
        eprintln!("no notes match");
        return ExitCode::from(1);
    }
    // Deterministic-ish shuffle using SystemTime as the
    // seed. Cheaper than pulling in `rand`.
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xDEADBEEF);
    let mut rng_state: u64 = seed.wrapping_add(1);
    let mut next = || -> u64 {
        rng_state = rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        rng_state
    };
    let n = pool.len();
    let mut order: Vec<usize> = (0..n).collect();
    // Fisher-Yates with our LCG.
    for i in (1..n).rev() {
        let j = (next() as usize) % (i + 1);
        order.swap(i, j);
    }
    let take = count.min(n);
    for &i in order.iter().take(take) {
        println!("{}", pool[i].display());
    }
    ExitCode::SUCCESS
}
