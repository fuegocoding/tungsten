//! Canvas data model — the JSON Canvas spec.
//!
//! Canvas files (`.canvas`) are JSON documents that hold a free-form
//! 2D arrangement of cards connected by labeled edges. The format
//! is the [JSON Canvas 1.0 spec][1] used by Obsidian.
//!
//! [1]: https://jsoncanvas.org/spec/1.0/
//!
//! **Scope of this module:**
//! - Type definitions that mirror the JSON Canvas schema
//! - `Canvas::from_str` / `Canvas::to_string` for parsing and
//!   serializing the on-disk representation
//! - A handful of helpers: add_node, add_edge, add_group, find
//!
//! **Out of scope:**
//! - Rendering (that's GPUI editor work; lives in the `zed`
//!   crate and the future `tungsten_canvas` crate)
//! - File-watching (the existing `NoteWatcher` covers .md; a
//!   sibling watcher for .canvas is a small follow-up)
//! - Embedding canvases in notes (handled by the parser's
//!   `![[My.canvas]]` support; rendering is editor work)

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Position in canvas coordinates (logical pixels, top-left
/// origin, y increases downward).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CanvasPosition {
    pub x: f64,
    pub y: f64,
}

/// Width and height in canvas coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CanvasDimensions {
    pub width: f64,
    pub height: f64,
}

/// Color stored as a hex string in any CSS-compatible form
/// (named, hex `#abcdef`, `rgb(...)`, etc.). Optional.
pub type Color = String;

/// A card on the canvas. Variants correspond to the JSON Canvas
/// `type` field: `text`, `file`, `link`, `group`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CanvasNode {
    /// Free-form text card.
    Text(TextNode),
    /// Reference to a file (note, image, audio, video, PDF).
    File(FileNode),
    /// Reference to an external URL.
    Link(LinkNode),
    /// Visual grouping (no content of its own).
    Group(GroupNode),
}

impl CanvasNode {
    pub fn id(&self) -> &str {
        match self {
            CanvasNode::Text(n) => &n.base.id,
            CanvasNode::File(n) => &n.base.id,
            CanvasNode::Link(n) => &n.base.id,
            CanvasNode::Group(n) => &n.base.id,
        }
    }
    pub fn position(&self) -> Option<CanvasPosition> {
        match self {
            CanvasNode::Text(n) => n.base.position(),
            CanvasNode::File(n) => n.base.position(),
            CanvasNode::Link(n) => n.base.position(),
            CanvasNode::Group(n) => n.base.position(),
        }
    }
    pub fn set_position(&mut self, p: CanvasPosition) {
        match self {
            CanvasNode::Text(n) => n.base.x = Some(p.x),
            CanvasNode::File(n) => n.base.x = Some(p.x),
            CanvasNode::Link(n) => n.base.x = Some(p.x),
            CanvasNode::Group(n) => n.base.x = Some(p.x),
        }
        let p2 = match self {
            CanvasNode::Text(n) => &mut n.base,
            CanvasNode::File(n) => &mut n.base,
            CanvasNode::Link(n) => &mut n.base,
            CanvasNode::Group(n) => &mut n.base,
        };
        p2.y = Some(p.y);
    }
}

/// Common fields for all node types (id + optional geometry).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeBase {
    pub id: String,
    #[serde(rename = "x", skip_serializing_if = "Option::is_none", default)]
    pub x: Option<f64>,
    #[serde(rename = "y", skip_serializing_if = "Option::is_none", default)]
    pub y: Option<f64>,
    #[serde(rename = "width", skip_serializing_if = "Option::is_none", default)]
    pub width: Option<f64>,
    #[serde(rename = "height", skip_serializing_if = "Option::is_none", default)]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub color: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub label: Option<String>,
}

impl NodeBase {
    pub fn position(&self) -> Option<CanvasPosition> {
        match (self.x, self.y) {
            (Some(x), Some(y)) => Some(CanvasPosition { x, y }),
            _ => None,
        }
    }
    pub fn dimensions(&self) -> Option<CanvasDimensions> {
        match (self.width, self.height) {
            (Some(width), Some(height)) => Some(CanvasDimensions { width, height }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextNode {
    #[serde(flatten)]
    pub base: NodeBase,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileNode {
    #[serde(flatten)]
    pub base: NodeBase,
    /// Path to the file, relative to the canvas file or absolute.
    pub file: String,
    /// Subtype hint: image, video, audio, pdf, or other.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub subtype: Option<FileSubtype>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileSubtype {
    Image,
    Video,
    Audio,
    Pdf,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkNode {
    #[serde(flatten)]
    pub base: NodeBase,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupNode {
    /// Node base (id, optional x/y/width/height, color, label).
    #[serde(flatten)]
    pub base: NodeBase,
    /// Child node ids belonging to this group.
    #[serde(default)]
    pub children: Vec<String>,
}

/// Edge between two nodes. Can be directed (one-way arrow) or
/// undirected (no arrowheads).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasEdge {
    pub id: String,
    #[serde(rename = "fromNode")]
    pub from_node: String,
    #[serde(rename = "toNode")]
    pub to_node: String,
    #[serde(rename = "fromSide", default)]
    pub from_side: Option<Side>,
    #[serde(rename = "toSide", default)]
    pub to_side: Option<Side>,
    /// `from` or `to` or `none`. When omitted, treated as `none`
    /// (undirected).
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub color: Option<Color>,
}

/// Side of a node that an edge connects to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

/// Top-level Canvas document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Canvas {
    #[serde(default)]
    pub nodes: Vec<CanvasNode>,
    #[serde(default)]
    pub edges: Vec<CanvasEdge>,
    /// Per-document metadata, opaque key-value pairs. Used by
    /// editors to store viewport state, etc.
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl Canvas {
    /// Parse a Canvas JSON document. Tolerant: missing fields
    /// default to empty collections.
    pub fn from_str(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
    /// Serialize a Canvas to its on-disk JSON form. Pretty-printed.
    pub fn to_string_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    /// Serialize compactly (one line).
    pub fn to_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
    pub fn find_node(&self, id: &str) -> Option<&CanvasNode> {
        self.nodes.iter().find(|n| n.id() == id)
    }
    pub fn find_node_mut(&mut self, id: &str) -> Option<&mut CanvasNode> {
        self.nodes.iter_mut().find(|n| n.id() == id)
    }
    pub fn find_edge(&self, id: &str) -> Option<&CanvasEdge> {
        self.edges.iter().find(|e| e.id == id)
    }
    pub fn add_node(&mut self, node: CanvasNode) {
        self.nodes.push(node);
    }
    pub fn add_edge(&mut self, edge: CanvasEdge) {
        self.edges.push(edge);
    }
    /// All node ids referenced from any edge, for cross-checking.
    pub fn referenced_node_ids(&self) -> std::collections::BTreeSet<&str> {
        self.edges
            .iter()
            .flat_map(|e| [e.from_node.as_str(), e.to_node.as_str()])
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_text_node_json() -> &'static str {
        r#"{
            "nodes": [{
                "id": "n1",
                "type": "text",
                "x": 0,
                "y": 0,
                "width": 200,
                "height": 100,
                "text": "Hello"
            }]
        }"#
    }

    #[test]
    fn parse_minimal_text_node() {
        let canvas = Canvas::from_str(sample_text_node_json()).unwrap();
        assert_eq!(canvas.nodes.len(), 1);
        match &canvas.nodes[0] {
            CanvasNode::Text(n) => {
                assert_eq!(n.base.id, "n1");
                assert_eq!(n.text, "Hello");
                assert_eq!(n.base.x, Some(0.0));
                assert_eq!(n.base.y, Some(0.0));
            }
            _ => panic!("expected text node"),
        }
    }

    #[test]
    fn parse_empty_canvas() {
        let canvas = Canvas::from_str("{}").unwrap();
        assert!(canvas.nodes.is_empty());
        assert!(canvas.edges.is_empty());
        assert!(canvas.metadata.is_empty());
    }

    #[test]
    fn round_trip_text_node() {
        let canvas = Canvas::from_str(sample_text_node_json()).unwrap();
        let json = canvas.to_string_pretty().unwrap();
        let back = Canvas::from_str(&json).unwrap();
        assert_eq!(canvas, back);
    }

    #[test]
    fn parse_file_node() {
        let json = r#"{
            "nodes": [{
                "id": "f1",
                "type": "file",
                "x": 100,
                "y": 50,
                "file": "note.md",
                "subtype": "pdf"
            }]
        }"#;
        let canvas = Canvas::from_str(json).unwrap();
        match &canvas.nodes[0] {
            CanvasNode::File(n) => {
                assert_eq!(n.file, "note.md");
                assert_eq!(n.subtype, Some(FileSubtype::Pdf));
            }
            _ => panic!("expected file node"),
        }
    }

    #[test]
    fn parse_link_node() {
        let json = r#"{
            "nodes": [{
                "id": "l1",
                "type": "link",
                "url": "https://example.com"
            }]
        }"#;
        let canvas = Canvas::from_str(json).unwrap();
        assert!(matches!(&canvas.nodes[0], CanvasNode::Link(_)));
    }

    #[test]
    fn parse_group_node() {
        let json = r#"{
            "nodes": [{
                "id": "g1",
                "type": "group",
                "x": 0, "y": 0, "width": 400, "height": 300,
                "label": "Phase 1",
                "children": ["n1", "n2"]
            }]
        }"#;
        let canvas = Canvas::from_str(json).unwrap();
        match &canvas.nodes[0] {
            CanvasNode::Group(n) => {
                assert_eq!(n.base.label.as_deref(), Some("Phase 1"));
                assert_eq!(n.children, vec!["n1", "n2"]);
            }
            _ => panic!("expected group node"),
        }
    }

    #[test]
    fn parse_edge() {
        let json = r#"{
            "nodes": [],
            "edges": [{
                "id": "e1",
                "fromNode": "n1",
                "toNode": "n2",
                "label": "links to"
            }]
        }"#;
        let canvas = Canvas::from_str(json).unwrap();
        assert_eq!(canvas.edges.len(), 1);
        let e = &canvas.edges[0];
        assert_eq!(e.from_node, "n1");
        assert_eq!(e.to_node, "n2");
        assert_eq!(e.label.as_deref(), Some("links to"));
    }

    #[test]
    fn add_node_and_edge() {
        let mut canvas = Canvas::default();
        canvas.add_node(CanvasNode::Text(TextNode {
            base: NodeBase {
                id: "n1".into(),
                x: Some(0.0),
                y: Some(0.0),
                width: None,
                height: None,
                color: None,
                label: None,
            },
            text: "Hello".into(),
        }));
        canvas.add_edge(CanvasEdge {
            id: "e1".into(),
            from_node: "n1".into(),
            to_node: "n1".into(),
            from_side: None,
            to_side: None,
            label: None,
            color: None,
        });
        assert_eq!(canvas.nodes.len(), 1);
        assert_eq!(canvas.edges.len(), 1);
    }

    #[test]
    fn find_node_by_id() {
        let mut canvas = Canvas::default();
        canvas.add_node(CanvasNode::Text(TextNode {
            base: NodeBase {
                id: "target".into(),
                x: None,
                y: None,
                width: None,
                height: None,
                color: None,
                label: None,
            },
            text: "x".into(),
        }));
        assert!(canvas.find_node("target").is_some());
        assert!(canvas.find_node("missing").is_none());
    }

    #[test]
    fn referenced_node_ids() {
        let mut canvas = Canvas::default();
        canvas.add_node(CanvasNode::Text(TextNode {
            base: NodeBase {
                id: "a".into(),
                x: None,
                y: None,
                width: None,
                height: None,
                color: None,
                label: None,
            },
            text: "a".into(),
        }));
        canvas.add_node(CanvasNode::Text(TextNode {
            base: NodeBase {
                id: "b".into(),
                x: None,
                y: None,
                width: None,
                height: None,
                color: None,
                label: None,
            },
            text: "b".into(),
        }));
        canvas.add_edge(CanvasEdge {
            id: "e1".into(),
            from_node: "a".into(),
            to_node: "b".into(),
            from_side: None,
            to_side: None,
            label: None,
            color: None,
        });
        let refs = canvas.referenced_node_ids();
        assert!(refs.contains("a"));
        assert!(refs.contains("b"));
    }
}
