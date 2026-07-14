//! `tungsten-graph` — compute a force-directed layout for a vault
//! and print node positions.
//!
//! Usage:
//!     tungsten-graph <VAULT_PATH> [--width=N] [--height=N]
//!         [--iterations=N] [--seed=N]
//!
//! Reads the vault's link graph (one node per note, one edge per
//! resolved wikilink) and runs a Fruchterman–Reingold simulation
//! to place every node. The result is a list of
//!
//!     path<TAB>title<TAB>x<TAB>y
//!
//! lines, with x and y in normalized `[0, 1]` coordinates. Pipe
//! to `awk` or your own renderer.
//!
//! Examples:
//!     tungsten-graph ~/Notes | head
//!     tungsten-graph ~/Notes --width=1200 --height=800 | sort -k4 -n
//!
//! Exit codes:
//!     0  layout produced
//!     2  bad arguments or index error

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{build_graph, layout, ForceParams, NoteIndex};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-graph <VAULT_PATH> [options]\n\
             \n\
             Options:\n\
               --iterations=N    simulation iterations (default 300)\n\
               --seed=N          RNG seed for reproducibility (default 0xC0FFEE)\n\
               --repulsion=F     repulsion strength (default 0.5)\n\
               --attraction=F    attraction strength (default 0.05)\n\
               \n\
             Output: one line per note:\n  \
                 path<TAB>title<TAB>x<TAB>y\n\
             x and y are normalized to [0, 1]."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }

    let mut params = ForceParams::default();
    for arg in &args[2..] {
        if let Some(rest) = arg.strip_prefix("--iterations=") {
            if let Ok(n) = rest.parse() {
                params.iterations = n;
            }
        } else if let Some(rest) = arg.strip_prefix("--seed=") {
            if let Ok(n) = rest.parse() {
                params.seed = n;
            }
        } else if let Some(rest) = arg.strip_prefix("--repulsion=") {
            if let Ok(n) = rest.parse() {
                params.repulsion = n;
            }
        } else if let Some(rest) = arg.strip_prefix("--attraction=") {
            if let Ok(n) = rest.parse() {
                params.attraction = n;
            }
        }
    }

    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(2);
        }
    };

    let (nodes, edges) = build_graph(&index);
    let result = layout(nodes, edges, params);
    for node in &result.nodes {
        if let Some(pos) = result.positions.get(&node.path) {
            println!(
                "{}\t{}\t{:.4}\t{:.4}",
                node.path.display(),
                node.title,
                pos.x,
                pos.y
            );
        }
    }
    ExitCode::SUCCESS
}
