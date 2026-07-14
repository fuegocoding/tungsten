//! Sidebar panel data layer (M2.1).
//!
//! Each panel type is a tagged enum that the GPUI view layer
//! iterates over. The data is computed from a [`NoteIndex`] and
//! (for note-specific panels) an optional `current_note_path`.
//! Rendering is the GPUI side's job; this module only gathers
//! and pre-sorts the data so the view doesn't have to walk the
//! index on every repaint.
//!
//! The panel types are intentionally compact: just enough
//! information for a list/row renderer. Anything heavier
//! (preview rendering, link hover, etc.) belongs in the view
//! crate, not here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::index::NoteIndex;
use crate::note::Note;
use crate::outline::{outline, Heading};

/// All sidebar panel kinds. The view layer dispatches on this
/// enum to pick a renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    FileTree,
    Search,
    Tags,
    Outline,
    Backlinks,
    Properties,
    Bookmarks,
    Graph,
}

impl Panel {
    /// Human-readable title for the panel header.
    pub fn title(&self) -> &'static str {
        match self {
            Self::FileTree => "Files",
            Self::Search => "Search",
            Self::Tags => "Tags",
            Self::Outline => "Outline",
            Self::Backlinks => "Backlinks",
            Self::Properties => "Properties",
            Self::Bookmarks => "Bookmarks",
            Self::Graph => "Graph",
        }
    }
}

/// A single row in the file tree: either a folder or a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTreeRow {
    pub path: PathBuf,
    pub label: String,
    pub is_folder: bool,
    pub depth: usize,
    pub note_count: Option<usize>,
}

/// All rows in the file tree, in display order (folders first,
/// then notes, sorted alphabetically within each folder).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileTreeData {
    pub rows: Vec<FileTreeRow>,
}

impl FileTreeData {
    /// Number of total notes (excluding folders).
    pub fn note_count(&self) -> usize {
        self.rows.iter().filter(|r| !r.is_folder).count()
    }
}

/// A single tag row: tag name, total notes, and the list of
/// note paths in that tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagRow {
    pub name: String,
    pub count: usize,
    pub notes: Vec<PathBuf>,
}

/// Tag panel data: rows sorted by count desc, then name asc.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagData {
    pub rows: Vec<TagRow>,
}

/// Backlink panel data for the current note.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BacklinkData {
    pub current: Option<PathBuf>,
    pub rows: Vec<PathBuf>,
}

/// Properties panel data: frontmatter rendered as a list of
/// key/value rows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropertyRow {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropertiesData {
    pub current: Option<PathBuf>,
    pub rows: Vec<PropertyRow>,
}

/// Bookmark panel data: notes marked with the `bookmarked`
/// frontmatter key (boolean true).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BookmarkData {
    pub rows: Vec<PathBuf>,
}

/// File tree for a vault.
pub fn file_tree(index: &NoteIndex) -> FileTreeData {
    // Group notes by their parent folder, then walk the
    // folders in sorted order.
    let mut by_folder: BTreeMap<PathBuf, Vec<&Note>> = BTreeMap::new();
    for note in index.notes() {
        let parent = note
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(""));
        by_folder.entry(parent).or_default().push(note);
    }

    let mut rows: Vec<FileTreeRow> = Vec::new();
    let mut folders_emitted: std::collections::BTreeSet<PathBuf> =
        std::collections::BTreeSet::new();
    for (folder, mut notes) in by_folder {
        // Emit any ancestor folders.
        let mut ancestors: Vec<PathBuf> = Vec::new();
        let mut current = folder.clone();
        loop {
            ancestors.push(current.clone());
            match current.parent() {
                Some(p) if !p.as_os_str().is_empty() => current = p.to_path_buf(),
                _ => break,
            }
        }
        for anc in ancestors.iter().rev() {
            if folders_emitted.insert(anc.clone()) {
                let depth = anc.components().count().saturating_sub(1);
                rows.push(FileTreeRow {
                    path: anc.clone(),
                    label: anc
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("/")
                        .to_string(),
                    is_folder: true,
                    depth,
                    note_count: None,
                });
            }
        }

        // Sort notes within the folder.
        notes.sort_by(|a, b| a.path.cmp(&b.path));
        let depth = folder.components().count().saturating_sub(1);
        for note in notes {
            rows.push(FileTreeRow {
                path: note.path.clone(),
                label: note
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?")
                    .to_string(),
                is_folder: false,
                depth,
                note_count: None,
            });
        }
    }
    FileTreeData { rows }
}

/// Tag panel data.
pub fn tags(index: &NoteIndex) -> TagData {
    let mut by_tag: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for note in index.notes() {
        for tag in &note.tags {
            by_tag.entry(tag.clone()).or_default().push(note.path.clone());
        }
    }
    let mut rows: Vec<TagRow> = by_tag
        .into_iter()
        .map(|(name, mut notes)| {
            notes.sort();
            notes.dedup();
            TagRow {
                count: notes.len(),
                notes,
                name,
            }
        })
        .collect();
    rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    TagData { rows }
}

/// Backlink data for `current` (note path). If `current` is
/// `None`, returns an empty `BacklinkData`.
pub fn backlinks(index: &NoteIndex, current: Option<&Path>) -> BacklinkData {
    let Some(path) = current else {
        return BacklinkData::default();
    };
    let Some(note) = index.get(path) else {
        return BacklinkData {
            current: Some(path.to_path_buf()),
            ..Default::default()
        };
    };
    let mut rows: Vec<PathBuf> = index
        .backlinks(&note.title)
        .map(|n| n.path.clone())
        .collect();
    rows.sort();
    rows.dedup();
    BacklinkData {
        current: Some(path.to_path_buf()),
        rows,
    }
}

/// Properties data for `current`.
pub fn properties(index: &NoteIndex, current: Option<&Path>) -> PropertiesData {
    let Some(path) = current else {
        return PropertiesData::default();
    };
    let Some(note) = index.get(path) else {
        return PropertiesData {
            current: Some(path.to_path_buf()),
            ..Default::default()
        };
    };
    let mut rows: Vec<PropertyRow> = Vec::new();
    if let Some(map) = note.frontmatter.as_mapping() {
        for (k, v) in map {
            let key = k.as_str().unwrap_or("?").to_string();
            let value = yaml_value_to_string(v);
            rows.push(PropertyRow { key, value });
        }
    }
    rows.sort_by(|a, b| a.key.cmp(&b.key));
    PropertiesData {
        current: Some(path.to_path_buf()),
        rows,
    }
}

/// Bookmarks: notes with `bookmarked: true` in frontmatter.
pub fn bookmarks(index: &NoteIndex) -> BookmarkData {
    let mut rows: Vec<PathBuf> = index
        .notes()
        .filter(|n| {
            n.frontmatter
                .get("bookmarked")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .map(|n| n.path.clone())
        .collect();
    rows.sort();
    BookmarkData { rows }
}

/// Outline data for the current note.
pub fn outline_rows(note: &Note) -> Vec<Heading> {
    crate::outline::outline(&note.content)
}

fn yaml_value_to_string(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::Null => "null".into(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Sequence(seq) => {
            let parts: Vec<String> =
                seq.iter().map(yaml_value_to_string).collect();
            format!("[{}]", parts.join(", "))
        }
        serde_yaml::Value::Mapping(m) => {
            let parts: Vec<String> = m
                .iter()
                .map(|(k, v)| {
                    format!("{}: {}", k.as_str().unwrap_or("?"), yaml_value_to_string(v))
                })
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        serde_yaml::Value::Tagged(t) => yaml_value_to_string(&t.value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_index(dir: &Path) -> NoteIndex {
        fs::create_dir_all(dir.join("subfolder")).unwrap();
        fs::write(
            dir.join("Welcome.md"),
            "---\ntags: [intro, daily]\n---\n# Welcome\nBody.\n",
        )
        .unwrap();
        fs::write(
            dir.join("subfolder/Note.md"),
            "---\ntags: [intro]\nbookmarked: true\n---\n# Note\n[[Welcome]]\n",
        )
        .unwrap();
        fs::write(
            dir.join("subfolder/Other.md"),
            "---\ntags: [work]\n---\n# Other\nNo links.\n",
        )
        .unwrap();
        NoteIndex::build(dir).unwrap()
    }

    fn tempdir() -> std::path::PathBuf {
        let base = std::env::temp_dir();
        let unique = format!(
            "tungsten-panel-test-{}-{}",
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

    #[test]
    fn file_tree_includes_folders_and_notes() {
        let dir = tempdir();
        let index = make_index(&dir);
        let tree = file_tree(&index);
        // 3 notes, 1 folder, plus the root folder itself
        let notes = tree.rows.iter().filter(|r| !r.is_folder).count();
        let folders = tree.rows.iter().filter(|r| r.is_folder).count();
        assert_eq!(notes, 3);
        assert!(folders >= 1);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tags_sorted_by_count() {
        let dir = tempdir();
        let index = make_index(&dir);
        let data = tags(&index);
        // "intro" has 2 notes, the rest have 1. So intro
        // should be first.
        assert!(!data.rows.is_empty());
        assert_eq!(data.rows[0].name, "intro");
        assert_eq!(data.rows[0].count, 2);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn backlinks_resolve_target() {
        let dir = tempdir();
        let index = make_index(&dir);
        let welcome = dir.join("Welcome.md");
        let data = backlinks(&index, Some(&welcome));
        assert_eq!(data.current, Some(welcome.clone()));
        assert!(!data.rows.is_empty());
        // Note.md links to Welcome.
        assert!(data
            .rows
            .iter()
            .any(|p| p.ends_with("Note.md")));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn backlinks_empty_for_nonexistent_note() {
        let dir = tempdir();
        let index = make_index(&dir);
        let data = backlinks(&index, Some(&dir.join("Nope.md")));
        assert!(data.rows.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn properties_includes_all_frontmatter() {
        let dir = tempdir();
        let index = make_index(&dir);
        let note = dir.join("subfolder/Note.md");
        let data = properties(&index, Some(&note));
        let keys: Vec<&str> = data.rows.iter().map(|r| r.key.as_str()).collect();
        assert!(keys.contains(&"tags"));
        assert!(keys.contains(&"bookmarked"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bookmarks_returns_marked_notes() {
        let dir = tempdir();
        let index = make_index(&dir);
        let data = bookmarks(&index);
        assert_eq!(data.rows.len(), 1);
        assert!(data.rows[0].ends_with("Note.md"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn outline_rows_returns_headings() {
        let dir = tempdir();
        let index = make_index(&dir);
        let note = dir.join("Welcome.md");
        let n = index.get(&note).unwrap();
        // Use the body only (skip the frontmatter so the
        // closing `---` isn't misread as a Setext underline).
        let body_start = n
            .content
            .find("---\n")
            .map(|i| i + 4)
            .and_then(|i| n.content[i..].find("\n---\n").map(|j| i + j + 5))
            .unwrap_or(0);
        let body = &n.content[body_start..];
        let rows = outline(body);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "Welcome");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn panel_titles_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for p in [
            Panel::FileTree,
            Panel::Search,
            Panel::Tags,
            Panel::Outline,
            Panel::Backlinks,
            Panel::Properties,
            Panel::Bookmarks,
            Panel::Graph,
        ] {
            assert!(seen.insert(p.title().to_string()));
        }
    }
}
