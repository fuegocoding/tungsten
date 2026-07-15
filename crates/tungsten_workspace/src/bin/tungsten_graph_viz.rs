//! `tungsten-graph-viz` — emit Graphviz DOT for the link graph.
//!
//! Usage:
//!     tungsten-graph-viz <VAULT_PATH> [--orphans] [--unlinked]
//!
//! Prints a DOT document on stdout. Pipe to `dot -Tsvg`
//! to render:
//!
//!     tungsten-graph-viz ~/Notes | dot -Tsvg > graph.svg
//!
//! Options:
//!     --orphans    also include notes with no incoming
//!                  or outgoing links
//!     --unlinked   include notes that have no backlinks
//!                  in the vault
//!
//! The output uses `cite` style for normal notes and
//! `note` style for orphans; edges carry the link kind
//! as a label.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{build_graph, layout, ForceParams, NoteIndex};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-graph-viz <VAULT_PATH> [--orphans] [--unlinked]\n\
             \n\
             Emit Graphviz DOT for the link graph. Pipe to\n\
             `dot -Tsvg` to render an SVG."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let include_orphans = args.iter().any(|a| a == "--orphans");
    let include_unlinked = args.iter().any(|a| a == "--unlinked");

    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(2);
        }
    };
    let (nodes, edges) = build_graph(&index);

    let orphans: HashSet<PathBuf> = index
        .orphans()
        .into_iter()
        .map(|n| n.path.clone())
        .collect();

    println!("digraph Tungsten {{");
    println!("  rankdir=LR;");
    println!("  node [shape=box, style=rounded, fontname=\"sans-serif\"];");
    println!("  edge [color=\"#666\"];");
    println!();

    // Filter nodes.
    let visible: Vec<_> = nodes
        .iter()
        .filter(|n| {
            if !include_orphans && orphans.contains(&n.path) {
                return false;
            }
            if !include_unlinked && n.degree == 0 {
                return false;
            }
            true
        })
        .cloned()
        .collect();
    let visible_paths: HashSet<PathBuf> = visible.iter().map(|n| n.path.clone()).collect();

    // Layout: produce positions for the visible nodes.
    // We re-run the FR layout on a subgraph.
    let vis_nodes: Vec<_> = visible.clone();
    let vis_edges: Vec<_> = edges
        .iter()
        .filter(|e| visible_paths.contains(&e.source) && visible_paths.contains(&e.target))
        .cloned()
        .collect();
    let laid_out = layout(vis_nodes, vis_edges, ForceParams::default());

    for n in &visible {
        let id = dot_id(&n.path);
        let label = dot_escape(&n.title);
        let pos = laid_out.positions.get(&n.path);
        let pos_attr = match pos {
            Some(p) => format!("pos=\"{:.3},{:.3}!\"", p.x * 1000.0, p.y * 1000.0),
            None => String::new(),
        };
        let orphan = if orphans.contains(&n.path) { ",style=dashed" } else { "" };
        println!("  {id} [label=\"{label}\"{orphan} {pos_attr}];");
    }
    println!();
    for e in &edges {
        if !visible_paths.contains(&e.source) || !visible_paths.contains(&e.target) {
            continue;
        }
        let s = dot_id(&e.source);
        let t = dot_id(&e.target);
        println!("  {s} -> {t};");
    }
    println!("}}");
    ExitCode::SUCCESS
}

fn dot_id(p: &std::path::Path) -> String {
    let s = p.display().to_string();
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}

fn dot_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
}
