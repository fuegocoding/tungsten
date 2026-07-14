//! `tungsten-query` — run a DQL query against a vault and print
//! the result rows.
//!
//! Usage:
//!     tungsten-query <VAULT_PATH> "<DQL QUERY>"
//!
//! Examples:
//!     tungsten-query ~/Notes "LIST"
//!     tungsten-query ~/Notes "LIST FROM \"Journal\""
//!     tungsten-query ~/Notes "TABLE file.name, file.tags FROM #lang"
//!     tungsten-query ~/Notes 'LIST FROM #journal SORT file.name LIMIT 10'
//!
//! Exit codes: 0 on success, 1 on index error, 2 on bad arguments,
//! 3 on parse error.

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{
    dql_execute, dql_parse_query, DqlError, DqlRow, NoteIndex,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 || args[1] == "--help" || args[1] == "-h" {
        eprintln!("usage: tungsten-query <VAULT_PATH> \"<DQL QUERY>\"");
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    let query_text = &args[2];

    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(1);
        }
    };
    let query = match dql_parse_query(query_text) {
        Ok(q) => q,
        Err(e) => {
            eprintln!("parse error: {e}");
            return ExitCode::from(3);
        }
    };
    let result = dql_execute(&query, &index);
    let source = result.source_type.keyword();
    println!("{source}: {} row(s)", result.rows.len());
    for (i, row) in result.rows.iter().enumerate() {
        match row {
            DqlRow::Note(n) => {
                println!(
                    "  [{}] {} ({})",
                    i + 1,
                    n.title,
                    n.path
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default()
                );
            }
            DqlRow::Table(n, values) => {
                let cells: Vec<String> = values
                    .iter()
                    .map(|v| if v.contains(',') || v.contains('"') {
                        format!("\"{}\"", v.replace('"', "\"\""))
                    } else {
                        v.clone()
                    })
                    .collect();
                println!(
                    "  [{}] {} | {}",
                    i + 1,
                    n.path
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    cells.join(" | ")
                );
            }
        }
    }
    // Hint to the user when the DQL implementation didn't run
    // anything because of an unknown operator.
    let _ = DqlError::UnexpectedEof; // keep the import used
    ExitCode::SUCCESS
}
