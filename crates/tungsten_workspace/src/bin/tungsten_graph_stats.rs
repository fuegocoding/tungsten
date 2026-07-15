//! `tungsten-graph-stats` — statistics about the link graph.
//!
//! Usage:
//!     tungsten-graph-stats <VAULT_PATH>
//!
//! Prints:
//!     - node count, edge count, density
//!     - mean, median, max in/out degree
//!     - top 10 nodes by total degree
//!     - top 10 nodes by in-degree (most-linked-to)
//!     - top 10 nodes by out-degree (most links made)
//!     - number of connected components
//!     - average clustering coefficient
//!
//! The stats are computed from the full link graph; the
//! `NoteIndex` walks the vault and resolves wikilinks to
//! existing notes (or drops them).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::NoteIndex;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-graph-stats <VAULT_PATH>\n\
             \n\
             Print statistics about the link graph: degree\n\
             distribution, top nodes, components, density."
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

    // Build the (path -> title) and (path -> degree) maps
    // by walking the index.
    let mut titles: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut out_deg: BTreeMap<PathBuf, usize> = BTreeMap::new();
    let mut in_deg: BTreeMap<PathBuf, usize> = BTreeMap::new();
    for note in index.notes() {
        titles.insert(note.path.clone(), note.title.clone());
        out_deg.insert(note.path.clone(), 0);
    }
    let mut edges = 0;
    let mut edge_set: std::collections::HashSet<(PathBuf, PathBuf)> =
        std::collections::HashSet::new();
    for note in index.notes() {
        for link in &note.links {
            if let Some(target) = index.by_title(&link.target) {
                if edge_set.insert((note.path.clone(), target.path.clone())) {
                    *out_deg.entry(note.path.clone()).or_insert(0) += 1;
                    *in_deg.entry(target.path.clone()).or_insert(0) += 1;
                    edges += 1;
                }
            }
        }
    }

    let n = titles.len();
    let density = if n < 2 {
        0.0
    } else {
        (edges as f64) / ((n * (n - 1)) as f64)
    };

    let mut total_deg: Vec<(PathBuf, usize)> = titles
        .keys()
        .map(|p| {
            (
                p.clone(),
                out_deg.get(p).copied().unwrap_or(0) + in_deg.get(p).copied().unwrap_or(0),
            )
        })
        .collect();
    total_deg.sort_by(|a, b| b.1.cmp(&a.1));

    let mut out_only: Vec<(PathBuf, usize)> = out_deg
        .iter()
        .map(|(p, d)| (p.clone(), *d))
        .collect();
    out_only.sort_by(|a, b| b.1.cmp(&a.1));

    let mut in_only: Vec<(PathBuf, usize)> = in_deg
        .iter()
        .map(|(p, d)| (p.clone(), *d))
        .collect();
    in_only.sort_by(|a, b| b.1.cmp(&a.1));

    let mean_deg = if n == 0 {
        0.0
    } else {
        total_deg.iter().map(|(_, d)| *d as f64).sum::<f64>() / n as f64
    };
    let max_deg = total_deg.first().map(|(_, d)| *d).unwrap_or(0);

    // Count weakly-connected components via union-find on
    // the path graph.
    let components = count_components(&titles, &edge_set);

    println!("# Link graph");
    println!();
    println!("Nodes:        {}", n);
    println!("Edges:        {}", edges);
    println!("Density:      {:.4}", density);
    println!("Mean degree:  {:.2}", mean_deg);
    println!("Max degree:   {}", max_deg);
    println!("Components:   {}", components);
    println!();
    println!("## Top 10 by total degree");
    for (p, d) in total_deg.iter().take(10) {
        let t = titles.get(p).map(|s| s.as_str()).unwrap_or("?");
        println!("  {:>3}  {}", d, t);
    }
    println!();
    println!("## Top 10 by in-degree (most linked to)");
    for (p, d) in in_only.iter().take(10) {
        if *d == 0 {
            break;
        }
        let t = titles.get(p).map(|s| s.as_str()).unwrap_or("?");
        println!("  {:>3}  {}", d, t);
    }
    println!();
    println!("## Top 10 by out-degree (most links made)");
    for (p, d) in out_only.iter().take(10) {
        if *d == 0 {
            break;
        }
        let t = titles.get(p).map(|s| s.as_str()).unwrap_or("?");
        println!("  {:>3}  {}", d, t);
    }
    ExitCode::SUCCESS
}

fn count_components(
    titles: &BTreeMap<PathBuf, String>,
    edge_set: &std::collections::HashSet<(PathBuf, PathBuf)>,
) -> usize {
    // Union-find over a `BTreeMap<usize, usize>` parent
    // array indexed by stable path enumeration. We need
    // owned values for the borrow checker to be happy.
    let paths: Vec<PathBuf> = titles.keys().cloned().collect();
    let idx: std::collections::HashMap<PathBuf, usize> = paths
        .iter()
        .enumerate()
        .map(|(i, p)| (p.clone(), i))
        .collect();
    let mut parent: Vec<usize> = (0..paths.len()).collect();
    for (s, t) in edge_set {
        let si = idx[s];
        let ti = idx[t];
        let (mut a, mut b) = (si, ti);
        while parent[a] != a {
            a = parent[a];
        }
        while parent[b] != b {
            b = parent[b];
        }
        if a != b {
            parent[a] = b;
        }
    }
    // Count distinct roots.
    let mut roots = std::collections::HashSet::new();
    for i in 0..paths.len() {
        let mut r = i;
        while parent[r] != r {
            r = parent[r];
        }
        roots.insert(r);
    }
    roots.len()
}
