//! Bases — the `.base` data model + a query evaluator.
//!
//! Obsidian Bases is a database-style view layer over a vault:
//! `.base` files are JSON documents that describe a query
//! (filter / sort / group by frontmatter properties) plus a
//! presentation (one of: Table, List, Cards, Map, Calendar). The
//! PRD §5.9 outlines the full scope; this module implements a
//! first cut of the data model and the query evaluator that
//! produces rows from a [`NoteIndex`].
//!
//! **What this does:**
//! - Parses `.base` files (JSON) into a [`Base`] struct
//! - Evaluates a [`Base::view`] against a [`NoteIndex`]: each note
//!   becomes a [`BaseRow`], with its properties read from the
//!   frontmatter
//! - Supports filter, sort, and group-by operations on properties
//!   (including the special `file.*` namespace)
//! - Renders a `Table` view to a list of [`BaseRow`]s; rendering
//!   for List/Cards/Map/Calendar is the editor's job
//!
//! **What this does NOT do (yet):**
//! - Formulas and Functions (Dataview-style expressions; M3.x)
//! - Registering custom view types (M3 plugin API)
//! - The actual GUI rendering of any view type (M2.x editor work)
//!
//! [NoteIndex]: crate::NoteIndex

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::index::NoteIndex;
use crate::note::Note;

/// A Base document. The JSON shape is a subset of the
/// Obsidian Bases spec: `views` (one or more named views) and
/// `formulas` (custom computed properties).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Base {
    /// Display name of this base.
    #[serde(default)]
    pub name: String,
    /// One or more named views on this base. Most bases have a
    /// single "default" view; multi-view bases are supported.
    #[serde(default)]
    pub views: Vec<BaseView>,
    /// User-defined computed properties (Dataview-style formulas).
    /// The evaluator doesn't yet compute these; they're listed so
    /// callers can present them in the UI.
    #[serde(default)]
    pub formulas: Vec<NamedFormula>,
}

/// A single view on a base.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaseView {
    /// Display name of this view (e.g. "default", "by status").
    #[serde(default)]
    pub name: String,
    /// The kind of view to render.
    #[serde(rename = "type", default)]
    pub kind: BaseViewKind,
    /// Optional view-specific configuration.
    #[serde(default)]
    pub config: serde_json::Value,
    /// Filter: which notes belong to this view.
    #[serde(default)]
    pub filters: Vec<Filter>,
    /// Sort: order the rows.
    #[serde(default)]
    pub sort: Vec<Sort>,
    /// Group-by: bucket the rows by a property.
    #[serde(default)]
    pub group_by: Option<String>,
    /// Properties to show in the table view (column headers).
    #[serde(default)]
    pub properties: Vec<String>,
    /// Optional limit on the number of rows.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Kind of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BaseViewKind {
    #[default]
    Table,
    List,
    Cards,
    Map,
    Calendar,
    Kanban,
}

/// A filter condition: `property op value`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    pub property: String,
    pub op: FilterOp,
    pub value: serde_json::Value,
}

/// Filter operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FilterOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Contains,
    StartsWith,
    /// `is-empty` (property missing or null/empty).
    IsEmpty,
    /// `is-not-empty` (property present and non-empty).
    IsNotEmpty,
}

/// Sort key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sort {
    pub property: String,
    #[serde(default)]
    pub descending: bool,
}

/// User-defined named formula (placeholder; the evaluator does
/// not yet compute formulas — they show up in the UI as available
/// properties but their values are always null).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedFormula {
    pub name: String,
    pub expression: String,
}

/// One row in a base view: a note + the values of the
/// properties the view cares about.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseRow {
    pub note_path: std::path::PathBuf,
    pub title: String,
    /// Property name -> display string. Includes the
    /// `file.*` namespace and any explicit properties the view
    /// listed.
    pub properties: BTreeMap<String, String>,
    /// Optional group key (when the view is grouped).
    pub group_key: Option<String>,
}

impl Base {
    /// Load a Base from a `.base` JSON file.
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        serde_json::from_slice(&bytes).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid .base JSON: {e}"),
            )
        })
    }

    /// Serialize to a JSON string.
    pub fn to_string_pretty(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Find the first view named `name`, or the first view if
    /// `name` is `None`.
    pub fn view(&self, name: Option<&str>) -> Option<&BaseView> {
        match name {
            Some(n) => self.views.iter().find(|v| v.name == n),
            None => self.views.first(),
        }
    }
}

impl BaseView {
    /// Evaluate this view against `index` and return the rows.
    pub fn evaluate(&self, index: &NoteIndex) -> Vec<BaseRow> {
        let mut rows: Vec<BaseRow> = index
            .notes()
            .filter(|n| self.filters.iter().all(|f| filter_matches(f, n)))
            .map(|n| {
                let mut properties = BTreeMap::new();
                for prop in &self.properties {
                    let val = crate::dql::render_field(n, prop);
                    properties.insert(prop.clone(), val);
                }
                let group_key = self
                    .group_by
                    .as_deref()
                    .map(|p| crate::dql::render_field(n, p));
                BaseRow {
                    note_path: n.path.clone(),
                    title: n.title.clone(),
                    properties,
                    group_key,
                }
            })
            .collect();
        // Sort.
        if !self.sort.is_empty() {
            rows.sort_by(|a, b| {
                for s in &self.sort {
                    let av = a
                        .properties
                        .get(&s.property)
                        .cloned()
                        .unwrap_or_default();
                    let bv = b
                        .properties
                        .get(&s.property)
                        .cloned()
                        .unwrap_or_default();
                    let cmp = av.cmp(&bv);
                    let cmp = if s.descending { cmp.reverse() } else { cmp };
                    if cmp != std::cmp::Ordering::Equal {
                        return cmp;
                    }
                }
                std::cmp::Ordering::Equal
            });
        }
        // Limit.
        if let Some(n) = self.limit {
            rows.truncate(n);
        }
        rows
    }
}

fn filter_matches(filter: &Filter, note: &Note) -> bool {
    let value = crate::dql::render_field(note, &filter.property);
    let target = match &filter.value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        _ => filter.value.to_string(),
    };
    let case_insensitive = |s: &str| s.to_lowercase();
    match filter.op {
        FilterOp::Eq => value == target,
        FilterOp::Ne => value != target,
        FilterOp::Lt => value < target,
        FilterOp::Le => value <= target,
        FilterOp::Gt => value > target,
        FilterOp::Ge => value >= target,
        FilterOp::Contains => case_insensitive(&value).contains(&case_insensitive(&target)),
        FilterOp::StartsWith => case_insensitive(&value).starts_with(&case_insensitive(&target)),
        FilterOp::IsEmpty => value.is_empty(),
        FilterOp::IsNotEmpty => !value.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_vault(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("tungsten-bases-{label}-{pid}-{n}"));
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir(dir.join(".obsidian")).unwrap();
        dir
    }

    fn write(dir: &Path, rel: &str, body: &str) {
        let path = dir.join(rel);
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(&path, body).unwrap();
    }

    #[test]
    fn parse_minimal_base() {
        let json = r#"{
            "name": "Tasks",
            "views": [{
                "name": "default",
                "type": "table",
                "properties": ["status", "due"]
            }]
        }"#;
        let base: Base = serde_json::from_str(json).unwrap();
        assert_eq!(base.name, "Tasks");
        assert_eq!(base.views.len(), 1);
        assert_eq!(base.views[0].kind, BaseViewKind::Table);
        assert_eq!(base.views[0].properties, vec!["status", "due"]);
    }

    #[test]
    fn parse_base_with_filters_and_sort() {
        let json = r#"{
            "views": [{
                "name": "default",
                "type": "table",
                "filters": [
                    {"property": "status", "op": "eq", "value": "draft"},
                    {"property": "tags", "op": "contains", "value": "rust"}
                ],
                "sort": [
                    {"property": "file.mtime", "descending": true}
                ],
                "limit": 10,
                "properties": ["status", "file.name"]
            }]
        }"#;
        let base: Base = serde_json::from_str(json).unwrap();
        let view = &base.views[0];
        assert_eq!(view.filters.len(), 2);
        assert_eq!(view.sort.len(), 1);
        assert!(view.sort[0].descending);
        assert_eq!(view.limit, Some(10));
    }

    #[test]
    fn evaluate_table_view_returns_matching_rows() {
        let dir = unique_vault("evaluate");
        write(
            &dir,
            "Draft A.md",
            "---\nstatus: draft\n---\n# Draft A\n",
        );
        write(
            &dir,
            "Draft B.md",
            "---\nstatus: draft\n---\n# Draft B\n",
        );
        write(
            &dir,
            "Published.md",
            "---\nstatus: published\n---\n# Published\n",
        );
        let idx = NoteIndex::build(&dir).unwrap();
        let base = Base {
            name: "All".into(),
            views: vec![BaseView {
                name: "default".into(),
                kind: BaseViewKind::Table,
                config: serde_json::Value::Null,
                filters: vec![Filter {
                    property: "status".into(),
                    op: FilterOp::Eq,
                    value: serde_json::Value::String("draft".into()),
                }],
                sort: Vec::new(),
                group_by: None,
                properties: vec!["status".into(), "file.name".into()],
                limit: None,
            }],
            formulas: Vec::new(),
        };
        let view = &base.views[0];
        let rows = view.evaluate(&idx);
        assert_eq!(rows.len(), 2);
        let names: Vec<&str> = rows.iter().map(|r| r.title.as_str()).collect();
        assert!(names.contains(&"Draft A"));
        assert!(names.contains(&"Draft B"));
        // Each row has the requested properties.
        for r in &rows {
            assert_eq!(r.properties.get("status").map(|s| s.as_str()), Some("draft"));
            assert!(r.properties.contains_key("file.name"));
        }
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn evaluate_sorts_by_property() {
        let dir = unique_vault("sort");
        write(&dir, "A.md", "---\norder: 3\n---\n# A\n");
        write(&dir, "B.md", "---\norder: 1\n---\n# B\n");
        write(&dir, "C.md", "---\norder: 2\n---\n# C\n");
        let idx = NoteIndex::build(&dir).unwrap();
        let view = BaseView {
            name: "default".into(),
            kind: BaseViewKind::Table,
            config: serde_json::Value::Null,
            filters: Vec::new(),
            sort: vec![Sort {
                property: "order".into(),
                descending: false,
            }],
            group_by: None,
            properties: vec!["order".into()],
            limit: None,
        };
        let rows = view.evaluate(&idx);
        let orders: Vec<&str> = rows
            .iter()
            .map(|r| r.properties.get("order").map(String::as_str).unwrap_or(""))
            .collect();
        assert_eq!(orders, vec!["1", "2", "3"]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn evaluate_applies_limit() {
        let dir = unique_vault("limit");
        for i in 0..5 {
            write(&dir, &format!("n{i}.md"), &format!("---\nk: {i}\n---\n# n{i}\n"));
        }
        let idx = NoteIndex::build(&dir).unwrap();
        let view = BaseView {
            name: "default".into(),
            kind: BaseViewKind::Table,
            config: serde_json::Value::Null,
            filters: Vec::new(),
            sort: Vec::new(),
            group_by: None,
            properties: vec!["k".into()],
            limit: Some(2),
        };
        let rows = view.evaluate(&idx);
        assert_eq!(rows.len(), 2);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn filter_is_empty_and_is_not_empty() {
        let dir = unique_vault("empty");
        write(&dir, "With status.md", "---\nstatus: draft\n---\n# With\n");
        write(&dir, "Without.md", "---\n---\n# Without\n");
        let idx = NoteIndex::build(&dir).unwrap();
        let view = BaseView {
            name: "default".into(),
            kind: BaseViewKind::Table,
            config: serde_json::Value::Null,
            filters: vec![Filter {
                property: "status".into(),
                op: FilterOp::IsEmpty,
                value: serde_json::Value::Null,
            }],
            sort: Vec::new(),
            group_by: None,
            properties: Vec::new(),
            limit: None,
        };
        let rows = view.evaluate(&idx);
        assert_eq!(rows.len(), 1, "expected only the 'Without' note");
        assert_eq!(rows[0].title, "Without");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn group_by_buckets_rows() {
        let dir = unique_vault("group");
        write(&dir, "A.md", "---\nstatus: draft\n---\n# A\n");
        write(&dir, "B.md", "---\nstatus: published\n---\n# B\n");
        write(&dir, "C.md", "---\nstatus: draft\n---\n# C\n");
        let idx = NoteIndex::build(&dir).unwrap();
        let view = BaseView {
            name: "default".into(),
            kind: BaseViewKind::Table,
            config: serde_json::Value::Null,
            filters: Vec::new(),
            sort: Vec::new(),
            group_by: Some("status".into()),
            properties: vec!["status".into()],
            limit: None,
        };
        let rows = view.evaluate(&idx);
        let draft = rows
            .iter()
            .filter(|r| r.group_key.as_deref() == Some("draft"))
            .count();
        let pub_ = rows
            .iter()
            .filter(|r| r.group_key.as_deref() == Some("published"))
            .count();
        assert_eq!(draft, 2);
        assert_eq!(pub_, 1);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn round_trip_base_json() {
        let base = Base {
            name: "Test".into(),
            views: vec![BaseView {
                name: "default".into(),
                kind: BaseViewKind::Cards,
                config: serde_json::Value::Null,
                filters: Vec::new(),
                sort: Vec::new(),
                group_by: None,
                properties: vec!["status".into()],
                limit: Some(5),
            }],
            formulas: Vec::new(),
        };
        let json = base.to_string_pretty().unwrap();
        let back: Base = serde_json::from_str(&json).unwrap();
        assert_eq!(base, back);
    }
}
