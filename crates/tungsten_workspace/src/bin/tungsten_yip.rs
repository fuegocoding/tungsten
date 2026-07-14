//! `tungsten-yip` — render a Year-in-Pixels SVG for a vault.
//!
//! Usage:
//!     tungsten-yip <VAULT_PATH> [--year=Y] [--out=PATH]
//!
//! Reads `mood:` and `energy:` from each daily note in the
//! `Journal/` folder and writes an SVG. Defaults to the
//! current year and stdout. Use `--out` to write to a file.
//!
//! Examples:
//!     tungsten-yip ~/Notes > ~/yip.svg
//!     tungsten-yip ~/Notes --year=2025 --out=/tmp/yip.svg

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{JournalConfig, NoteIndex, YipGrid, YipSvg};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-yip <VAULT_PATH> [--year=Y] [--out=PATH]\n\
             \n\
             Render a Year-in-Pixels SVG. With --out, write\n\
             to a file; otherwise print to stdout."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let mut year: i32 = {
        let now = chrono::Local::now().naive_local();
        now.format("%Y").to_string().parse().unwrap_or(2026)
    };
    let mut out_path: Option<PathBuf> = None;
    for a in &args[2..] {
        if let Some(rest) = a.strip_prefix("--year=") {
            if let Ok(y) = rest.parse() {
                year = y;
            }
        } else if let Some(rest) = a.strip_prefix("--out=") {
            out_path = Some(PathBuf::from(rest));
        }
    }

    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(2);
        }
    };
    let grid = YipGrid::for_year(&index, &JournalConfig::default(), year);
    let svg = YipSvg::render(&grid);
    eprintln!(
        "year {}: {} entries, {} with mood",
        year,
        grid.entries(),
        grid.filled()
    );
    match out_path {
        Some(p) => match std::fs::write(&p, &svg.svg) {
            Ok(_) => {
                eprintln!("wrote {}", p.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("write error: {e}");
                ExitCode::from(2)
            }
        },
        None => {
            println!("{}", svg.svg);
            ExitCode::SUCCESS
        }
    }
}
