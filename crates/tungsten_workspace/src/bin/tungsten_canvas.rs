//! `tungsten-canvas` — inspect or summarize a JSON Canvas file.
//!
//! Usage:
//!     tungsten-canvas <PATH>
//!         Parse a .canvas file and print a one-line summary
//!         per node and a list of edges. The summary includes id,
//!         type, position, and size (when present).
//!
//!     tungsten-canvas --summary <PATH>
//!         Just the totals (node count by type, edge count).

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::{Canvas, CanvasNode};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!(
            "usage: tungsten-canvas [--summary] <PATH>\n\
             \n\
             Parse a JSON Canvas file and print a per-node summary\n\
             plus the list of edges."
        );
        return ExitCode::from(2);
    }
    let summary_only = args.iter().any(|a| a == "--summary");
    let positional: Vec<&String> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    if positional.len() != 1 {
        eprintln!("usage: tungsten-canvas [--summary] <PATH>");
        return ExitCode::from(2);
    }
    let path = PathBuf::from(positional[0]);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read error: {e}");
            return ExitCode::from(1);
        }
    };
    let text = match std::str::from_utf8(&bytes) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("not utf-8: {e}");
            return ExitCode::from(1);
        }
    };
    let canvas = match Canvas::from_str(text) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("parse error: {e}");
            return ExitCode::from(1);
        }
    };

    if summary_only {
        let mut counts: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for n in &canvas.nodes {
            let t = match n {
                CanvasNode::Text(_) => "text",
                CanvasNode::File(_) => "file",
                CanvasNode::Link(_) => "link",
                CanvasNode::Group(_) => "group",
            };
            *counts.entry(t).or_insert(0) += 1;
        }
        println!("file:          {}", path.display());
        println!("nodes:         {}", canvas.nodes.len());
        for (t, n) in &counts {
            println!("  {}:         {}", t, n);
        }
        println!("edges:         {}", canvas.edges.len());
        return ExitCode::SUCCESS;
    }

    println!("file: {}", path.display());
    println!("nodes ({}):", canvas.nodes.len());
    for n in &canvas.nodes {
        let (ty, extra) = match n {
            CanvasNode::Text(t) => (
                "text",
                format!("text=\"{}\"", truncate(&t.text, 40)),
            ),
            CanvasNode::File(f) => (
                "file",
                format!(
                    "file=\"{}\" subtype={:?}",
                    f.file,
                    f.subtype
                ),
            ),
            CanvasNode::Link(l) => ("link", format!("url=\"{}\"", l.url)),
            CanvasNode::Group(g) => (
                "group",
                format!("children={}", g.children.len()),
            ),
        };
        let pos = n
            .position()
            .map(|p| format!("@({:.0},{:.0})", p.x, p.y))
            .unwrap_or_default();
        println!("  [{}] {} {}{pos}", n.id(), ty, extra);
    }
    println!("edges ({}):", canvas.edges.len());
    for e in &canvas.edges {
        let lbl = e
            .label
            .as_deref()
            .map(|s| format!(" \"{s}\""))
            .unwrap_or_default();
        println!("  [{}] {} -> {}{lbl}", e.id, e.from_node, e.to_node);
    }
    ExitCode::SUCCESS
}

fn truncate(s: &str, max: usize) -> String {
    let single = s.replace('\n', " ");
    if single.chars().count() <= max {
        single
    } else {
        let mut out: String = single.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
