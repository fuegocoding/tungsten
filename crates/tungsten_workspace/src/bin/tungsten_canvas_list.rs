//! `tungsten-canvas-list` — list every JSON Canvas file in
//! a vault.
//!
//! Usage:
//!     tungsten-canvas-list <VAULT_PATH> [--json]
//!
//! For each `*.canvas` file, prints:
//!     path<TAB>node_count<TAB>edge_count<TAB>title
//!
//! The title is taken from the first text node that has
//! a non-empty label, if any. The output is one row per
//! file; --json emits a single JSON array.
//!
//! Examples:
//!     tungsten-canvas-list ~/Notes
//!     tungsten-canvas-list ~/Notes --json

use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;
use tungsten_workspace::canvas::{Canvas, CanvasNode};

#[derive(Debug, Clone, Serialize)]
struct CanvasSummary {
    path: String,
    nodes: usize,
    edges: usize,
    title: Option<String>,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-canvas-list <VAULT_PATH> [--json]\n\
             \n\
             List every JSON Canvas file in a vault with\n\
             its node and edge counts."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let json = args.iter().any(|a| a == "--json");

    let entries: Vec<CanvasSummary> = walkdir::WalkDir::new(&vault)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("canvas"))
        .filter_map(|e| {
            let content = std::fs::read_to_string(e.path()).ok()?;
            let canvas: Canvas = serde_json::from_str(&content).ok()?;
            let title = first_text_label(&canvas);
            Some(CanvasSummary {
                path: e.path().display().to_string(),
                nodes: canvas.nodes.len(),
                edges: canvas.edges.len(),
                title,
            })
        })
        .collect();

    if json {
        match serde_json::to_string_pretty(&entries) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("json error: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        for s in &entries {
            println!(
                "{}\t{}\t{}\t{}",
                s.path,
                s.nodes,
                s.edges,
                s.title.as_deref().unwrap_or("")
            );
        }
    }
    ExitCode::SUCCESS
}

fn first_text_label(canvas: &Canvas) -> Option<String> {
    for node in &canvas.nodes {
        if let CanvasNode::Text(t) = node {
            if !t.text.is_empty() {
                return Some(t.text.clone());
            }
            if let Some(label) = &t.base.label {
                if !label.is_empty() {
                    return Some(label.clone());
                }
            }
        }
    }
    None
}
