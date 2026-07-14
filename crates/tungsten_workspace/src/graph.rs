//! Force-directed layout for the vault's link graph.
//!
//! The graph has a [`Node`] for every note in the [`NoteIndex`] and
//! an [`Edge`] for every wikilink between two notes that resolve to
//! a known title. The layout uses a simple Fruchterman–Reingold
//! simulation: nodes repel each other (like charges) and edges
//! pull their endpoints together (like springs). After a fixed
//! number of iterations the simulation is "cooled" and the
//! resulting positions are returned as [`Layout`].
//!
//! The output is deterministic given a fixed seed and the same
//! graph. Positions are normalized to the unit square `[0, 1]^2`
//! so the renderer can scale them to the available viewport.
//!
//! This is M1.2 of the roadmap ("graph layout"). The data
//! structures (nodes, edges) are also used by the M0.2 graph view
//! to decide what to draw and how to size it.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::index::NoteIndex;

/// A node in the link graph.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub path: PathBuf,
    pub title: String,
    /// Outgoing wikilink count. Used as a "weight" for sizing.
    pub degree: usize,
}

/// A directed edge: `source` links to `target`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub source: PathBuf,
    pub target: PathBuf,
}

/// A 2D position in the unit square.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

/// A complete graph layout: positioned nodes plus the original
/// edges. The renderer walks both to draw.
#[derive(Debug, Clone)]
pub struct Layout {
    pub positions: HashMap<PathBuf, Position>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Bounding box the positions were normalized to. Always
    /// `[0, 1]^2` unless the layout was constructed manually.
    pub bounds: (f32, f32, f32, f32),
}

impl Layout {
    /// Scale every position so the layout fits the given
    /// `(width, height)` viewport, returning positions keyed by
    /// path. The origin is `(0, 0)`, not centered.
    pub fn scaled(&self, width: f32, height: f32) -> HashMap<PathBuf, (f32, f32)> {
        self.positions
            .iter()
            .map(|(path, pos)| (path.clone(), (pos.x * width, pos.y * height)))
            .collect()
    }

    /// Number of nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True if the graph has no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Build a graph snapshot from a [`NoteIndex`].
///
/// Edges are emitted only when both endpoints resolve to existing
/// notes (by title lookup). Broken links are dropped — they would
/// be dangling in the rendered graph anyway, and the index's
/// `find_broken` helper exposes them separately.
pub fn build_graph(index: &NoteIndex) -> (Vec<Node>, Vec<Edge>) {
    let mut nodes: Vec<Node> = index
        .notes()
        .map(|note| {
            let degree = index
                .forward_links(&note.path)
                .len();
            Node {
                path: note.path.clone(),
                title: note.title.clone(),
                degree,
            }
        })
        .collect();
    nodes.sort_by(|a, b| a.path.cmp(&b.path));

    let mut edges = Vec::new();
    for note in index.notes() {
        for target in index.forward_links(&note.path) {
            edges.push(Edge {
                source: note.path.clone(),
                target: target.path.clone(),
            });
        }
    }
    (nodes, edges)
}

/// Tunable parameters for the force-directed simulation.
///
/// Defaults are tuned for a few hundred nodes. Larger vaults
/// should pass a larger `iterations` and a smaller `ideal_length`.
#[derive(Debug, Clone, Copy)]
pub struct ForceParams {
    /// Ideal edge length (in normalized units, where the
    /// `width`/`height` are `1.0`). Larger values spread the
    /// graph out more.
    pub ideal_length: f32,
    /// Strength of the repulsive force between every pair of
    /// nodes.
    pub repulsion: f32,
    /// Strength of the attractive force along edges.
    pub attraction: f32,
    /// Maximum number of simulation iterations.
    pub iterations: usize,
    /// Initial temperature. The simulation cools linearly
    /// toward zero over the iterations.
    pub initial_temperature: f32,
    /// Random seed for deterministic layouts.
    pub seed: u64,
}

impl Default for ForceParams {
    fn default() -> Self {
        Self {
            ideal_length: 0.05,
            repulsion: 0.5,
            attraction: 0.05,
            iterations: 300,
            initial_temperature: 0.1,
            seed: 0xC0FFEE,
        }
    }
}

/// Run a Fruchterman–Reingold layout and return a [`Layout`].
///
/// The output positions are normalized to the unit square. The
/// `seed` makes the result reproducible.
pub fn layout(nodes: Vec<Node>, edges: Vec<Edge>, params: ForceParams) -> Layout {
    let n = nodes.len();
    if n == 0 {
        return Layout {
            positions: HashMap::new(),
            nodes,
            edges,
            bounds: (0.0, 0.0, 1.0, 1.0),
        };
    }

    // Seeded LCG so we don't pull in `rand`. Stay in u32 to
    // avoid f32 precision loss when casting large u64 values.
    let mut rng_state: u32 = (params.seed as u32).wrapping_add(1).max(1);
    let mut next_rand = || -> f32 {
        rng_state = rng_state
            .wrapping_mul(1664525)
            .wrapping_add(1013904223);
        (rng_state as f32) / (u32::MAX as f32)
    };

    let mut positions: Vec<(f32, f32)> = (0..n)
        .map(|_| (next_rand(), next_rand()))
        .collect();

    // Standard FR: k = C * sqrt(area / n). C defaults to the
    // user-supplied `ideal_length` so the same `C` is used as
    // the natural edge length.
    let area = 1.0_f32;
    let k = params.ideal_length * (area / (n as f32)).sqrt();
    let k2 = k * k;

    for it in 0..params.iterations {
        let temperature = params.initial_temperature
            * (1.0 - (it as f32) / (params.iterations as f32));

        let mut disp: Vec<(f32, f32)> = vec![(0.0, 0.0); n];

        // Repulsive force between every pair of nodes.
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = positions[i].0 - positions[j].0;
                let dy = positions[i].1 - positions[j].1;
                let dist = (dx * dx + dy * dy).sqrt().max(0.0001);
                let force = params.repulsion * k2 / dist;
                let fx = (dx / dist) * force;
                let fy = (dy / dist) * force;
                disp[i].0 += fx;
                disp[i].1 += fy;
                disp[j].0 -= fx;
                disp[j].1 -= fy;
            }
        }

        // Attractive force along edges.
        for edge in &edges {
            let Some(i) = node_index(&nodes, &edge.source) else { continue };
            let Some(j) = node_index(&nodes, &edge.target) else { continue };
            let dx = positions[i].0 - positions[j].0;
            let dy = positions[i].1 - positions[j].1;
            let dist = (dx * dx + dy * dy).sqrt().max(0.0001);
            let force = (dist * dist) / k * params.attraction;
            let fx = (dx / dist) * force;
            let fy = (dy / dist) * force;
            disp[i].0 -= fx;
            disp[i].1 -= fy;
            disp[j].0 += fx;
            disp[j].1 += fy;
        }

        // Apply displacement, capped by the temperature.
        for i in 0..n {
            let mag = (disp[i].0 * disp[i].0 + disp[i].1 * disp[i].1).sqrt();
            let cap = mag.min(temperature);
            let scale = if mag > 0.0 { cap / mag } else { 0.0 };
            positions[i].0 += disp[i].0 * scale;
            positions[i].1 += disp[i].1 * scale;
            // Keep nodes inside the unit square with a small
            // margin.
            let m = 0.01;
            positions[i].0 = positions[i].0.clamp(m, 1.0 - m);
            positions[i].1 = positions[i].1.clamp(m, 1.0 - m);
        }
    }

    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for &(x, y) in &positions {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    // Avoid divide-by-zero in the trivial one-node case.
    let w = (max_x - min_x).max(1e-6);
    let h = (max_y - min_y).max(1e-6);
    let mut normalized: Vec<(f32, f32)> = Vec::with_capacity(n);
    for &(x, y) in &positions {
        normalized.push(((x - min_x) / w, (y - min_y) / h));
    }

    let mut position_map = HashMap::with_capacity(n);
    for (i, node) in nodes.iter().enumerate() {
        let (x, y) = normalized[i];
        position_map.insert(node.path.clone(), Position { x, y });
    }

    Layout {
        positions: position_map,
        nodes,
        edges,
        bounds: (0.0, 0.0, 1.0, 1.0),
    }
}

fn node_index(nodes: &[Node], path: &PathBuf) -> Option<usize> {
    // Linear search; for >10k notes a HashMap is faster, but
    // the FR loop dominates and the lookup is cheap enough.
    nodes.iter().position(|n| n.path == *path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::NoteIndex;
    use std::fs;
    use std::path::Path;

    fn make_index(dir: &Path) -> NoteIndex {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("A.md"), "# A\nLinks to [[B]] and [[C]].\n").unwrap();
        fs::write(dir.join("B.md"), "# B\nBack to [[A]].\n").unwrap();
        fs::write(dir.join("C.md"), "# C\nOrphan.\n").unwrap();
        NoteIndex::build(dir).unwrap()
    }

    #[test]
    fn build_graph_captures_nodes_and_edges() {
        let dir = tempdir();
        let index = make_index(&dir);
        let (nodes, edges) = build_graph(&index);
        assert_eq!(nodes.len(), 3);
        let titles: Vec<&str> = nodes.iter().map(|n| n.title.as_str()).collect();
        assert!(titles.contains(&"A"));
        assert!(titles.contains(&"B"));
        assert!(titles.contains(&"C"));
        // A -> B, A -> C, B -> A
        assert_eq!(edges.len(), 3);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn layout_normalizes_to_unit_square() {
        let dir = tempdir();
        let index = make_index(&dir);
        let (nodes, edges) = build_graph(&index);
        let l = layout(nodes, edges, ForceParams::default());
        for pos in l.positions.values() {
            assert!(pos.x >= 0.0 && pos.x <= 1.0, "x out of range: {}", pos.x);
            assert!(pos.y >= 0.0 && pos.y <= 1.0, "y out of range: {}", pos.y);
        }
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn layout_is_deterministic_for_same_seed() {
        let dir = tempdir();
        let index = make_index(&dir);
        let (n1, e1) = build_graph(&index);
        let (n2, e2) = build_graph(&index);
        let a = layout(n1, e1, ForceParams::default());
        let b = layout(n2, e2, ForceParams::default());
        for (path, pa) in &a.positions {
            let pb = b.positions.get(path).unwrap();
            assert!((pa.x - pb.x).abs() < 1e-5);
            assert!((pa.y - pb.y).abs() < 1e-5);
        }
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn layout_handles_empty_graph() {
        let l = layout(vec![], vec![], ForceParams::default());
        assert!(l.is_empty());
    }

    #[test]
    fn layout_handles_single_node() {
        let n = Node {
            path: PathBuf::from("/x/A.md"),
            title: "A".into(),
            degree: 0,
        };
        let l = layout(vec![n], vec![], ForceParams::default());
        assert_eq!(l.len(), 1);
        let pos = l.positions.get(&PathBuf::from("/x/A.md")).unwrap();
        assert!(pos.x >= 0.0 && pos.x <= 1.0);
        assert!(pos.y >= 0.0 && pos.y <= 1.0);
    }

    #[test]
    fn scaled_maps_to_viewport() {
        let dir = tempdir();
        let index = make_index(&dir);
        let (nodes, edges) = build_graph(&index);
        let l = layout(nodes, edges, ForceParams::default());
        let scaled = l.scaled(800.0, 600.0);
        for (_, (x, y)) in &scaled {
            assert!(*x >= 0.0 && *x <= 800.0);
            assert!(*y >= 0.0 && *y <= 600.0);
        }
        fs::remove_dir_all(&dir).ok();
    }

    fn tempdir() -> std::path::PathBuf {
        let base = std::env::temp_dir();
        let unique = format!(
            "tungsten-graph-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let p = base.join(unique);
        fs::create_dir_all(&p).unwrap();
        p
    }
}
