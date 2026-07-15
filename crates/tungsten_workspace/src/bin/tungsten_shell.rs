//! `tungsten-shell` — interactive DQL REPL.
//!
//! Usage:
//!     tungsten-shell <VAULT_PATH>
//!
//! Reads DQL queries from stdin, one per line, and prints
//! the result rows. Special commands:
//!
//!     .help           print this help
//!     .quit           exit
//!     .exit           exit
//!     .tables         list available table-style queries
//!     .index          show index statistics
//!     .tags           list all tags with counts
//!     .orphans        list orphan notes
//!
//! Each non-special line is parsed and executed as a DQL
//! query. The result is printed as `path<TAB>title<TAB>
//! tag1,tag2,...` rows.
//!
//! Examples:
//!     tungsten-shell ~/Notes
//!     > LIST FROM #work
//!     > TABLE file.mtime, file.size WHERE file.mtime > "2026-01-01"

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{dql_execute, dql_parse_query, DqlRow, NoteIndex};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-shell <VAULT_PATH>\n\
             \n\
             Interactive DQL REPL. Reads queries from stdin."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(2);
        }
    };

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut stdout_lock = stdout.lock();
    let _ = writeln!(
        stdout_lock,
        "tungsten-shell: {} notes, {} links, {} tags. Type .help",
        index.len(),
        index.stats().link_count,
        index.stats().tag_count,
    );
    let _ = stdout_lock.flush();

    let mut line = String::new();
    loop {
        line.clear();
        let _ = write!(stdout_lock, "> ");
        let _ = stdout_lock.flush();
        let n = match stdin.lock().read_line(&mut line) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("read error: {e}");
                return ExitCode::from(2);
            }
        };
        if n == 0 {
            // EOF
            return ExitCode::SUCCESS;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match trimmed {
            ".quit" | ".exit" => return ExitCode::SUCCESS,
            ".help" => print_help(&mut stdout_lock),
            ".tables" => print_tables(&mut stdout_lock),
            ".index" => print_index(&index, &mut stdout_lock),
            ".tags" => print_tags(&index, &mut stdout_lock),
            ".orphans" => print_orphans(&index, &mut stdout_lock),
            _ => run_query(trimmed, &index, &mut stdout_lock),
        }
        let _ = stdout_lock.flush();
    }
}

fn print_help(out: &mut impl Write) {
    let _ = writeln!(
        out,
        "Commands:\n  \
         .help            this help\n  \
         .quit / .exit    exit the shell\n  \
         .tables          list table-style query types\n  \
         .index           show index statistics\n  \
         .tags            list all tags with counts\n  \
         .orphans         list orphan notes\n\n\
         Any other line is a DQL query, e.g.:\n  \
         LIST FROM #work\n  \
         TABLE file.mtime WHERE file.mtime > \"2026-01-01\""
    );
}

fn print_tables(out: &mut impl Write) {
    let _ = writeln!(
        out,
        "Available result kinds:\n  \
         LIST   path<TAB>title<TAB>tags\n  \
         TABLE  select columns with WHERE clauses"
    );
}

fn print_index(index: &NoteIndex, out: &mut impl Write) {
    let s = index.stats();
    let _ = writeln!(
        out,
        "notes: {}, links: {}, tags: {}, orphans: {}",
        s.note_count, s.link_count, s.tag_count, s.orphan_count
    );
}

fn print_tags(index: &NoteIndex, out: &mut impl Write) {
    let mut tags: Vec<(&str, usize)> = index
        .tags()
        .map(|t| (t, index.with_tag(t).count()))
        .collect();
    tags.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    for (t, c) in tags {
        let _ = writeln!(out, "  {c:>4}  #{t}");
    }
}

fn print_orphans(index: &NoteIndex, out: &mut impl Write) {
    for note in index.orphans() {
        let _ = writeln!(out, "  {}", note.path.display());
    }
}

fn run_query(line: &str, index: &NoteIndex, out: &mut impl Write) {
    let q = match dql_parse_query(line) {
        Ok(q) => q,
        Err(e) => {
            let _ = writeln!(out, "parse error: {e}");
            return;
        }
    };
    let result = dql_execute(&q, index);
    if result.rows.is_empty() {
        let _ = writeln!(out, "(0 rows)");
        return;
    }
    let _ = writeln!(out, "{} row(s):", result.rows.len());
    for row in &result.rows {
        let note = row.note();
        let _ = writeln!(
            out,
            "  {}\t{}\t{}",
            note.path.display(),
            note.title,
            note.tags.join(",")
        );
        if let DqlRow::Table(_, fields) = row {
            for f in fields {
                let _ = writeln!(out, "    | {f}");
            }
        }
    }
}
