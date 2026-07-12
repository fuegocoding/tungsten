//! Note model and markdown parser.
//!
//! A [`Note`] is the in-memory representation of a single `.md` file
//! in a vault: its path, parsed title, raw content, extracted
//! frontmatter, outgoing links, and tags. Notes are produced by
//! [`parse_note`] from a file on disk and consumed by [`NoteIndex`]
//! to build the link graph.
//!
//! Obsidian's markdown extensions (wikilinks, tags, block refs) are
//! not part of CommonMark, so we layer a small regex-based scanner
//! on top of `pulldown-cmark`. The scanner is intentionally
//! permissive — Obsidian's exact syntax is documented in places and
//! reverse-engineered in others; matching what Obsidian does is the
//! right bar for "drop-in compatibility."

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Deserialize;

use crate::note_parser::ParsedNote;

/// A single `.md` file in a vault, after parsing.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    /// Absolute path to the file on disk.
    pub path: PathBuf,

    /// Display title. Derived from the first H1 in the body, or from
    /// the filename (without `.md`) if there is no H1.
    pub title: String,

    /// The full raw markdown source as read from disk.
    pub content: String,

    /// YAML frontmatter, deserialized into an untyped `serde_yaml::Value`
    /// so callers can read whatever keys the user's vault uses
    /// (Obsidian does not enforce a schema).
    pub frontmatter: serde_yaml::Value,

    /// All wikilinks and markdown links extracted from the body.
    /// Inline references in code spans are excluded.
    pub links: Vec<Link>,

    /// Inline `#tag` and frontmatter `tags: [...]` entries,
    /// normalized to lowercase, without the leading `#`.
    pub tags: Vec<String>,

    /// Last-modified time, if the filesystem reported one.
    pub mtime: Option<SystemTime>,

    /// File size in bytes, used as a quick "has this changed" hint
    /// before the more expensive content hash.
    pub size_bytes: u64,
}

/// A link extracted from a note's body.
///
/// Covers Obsidian wikilinks (`[[Target]]`, `[[Target|alias]]`,
/// `[[Target#Heading]]`, `[[Target#^block]]`) and standard markdown
/// links (`[text](Target.md)`). The `target` is always the raw
/// target as it appears in the source; resolution to a path is
/// done by [`NoteIndex::by_title`] (case-insensitive) at query time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The raw target string (e.g. `"MyNote"`, `"MyNote#Heading"`,
    /// `"folder/MyNote.md"`).
    pub target: String,
    /// The display alias if present (e.g. `[[MyNote|display]]`).
    pub alias: Option<String>,
    /// The kind of link.
    pub kind: LinkKind,
    /// Byte offset into the source `Note::content`.
    pub byte_range: Range<usize>,
}

/// The kind of link. Distinguishes Obsidian wikilinks from
/// standard markdown links and breaks wikilinks into their
/// sub-forms (heading, block, section).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// `[[Target]]` — plain wikilink.
    Wiki,
    /// `[[Target#Heading]]` — wikilink to a heading.
    WikiHeading,
    /// `[[Target#^block-id]]` — wikilink to a block.
    WikiBlock,
    /// `[[Target##header]]` — wikilink to a sub-heading.
    WikiSection,
    /// `[text](Target.md)` — standard markdown link to a `.md` file.
    Markdown,
}

/// An unlinked mention of a note's title found in another note's body.
///
/// Unlinked mentions are plain text occurrences of a note's title
/// that are *not* wrapped in a wikilink. They are surfaced by the
/// Backlinks panel and can be promoted to a real link with a
/// one-click action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlinkedMention {
    /// The note that contains the mention.
    pub source: PathBuf,
    /// Byte offset into the source's content.
    pub byte_range: Range<usize>,
}

impl Note {
    /// Read a note from disk. The file is fully loaded into memory;
    /// for very large files a streaming parser would be preferable,
    /// but `.md` files in note-taking vaults are typically <100 KB.
    pub fn read(path: &Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let metadata = std::fs::metadata(path).ok();
        let mtime = metadata.as_ref().and_then(|m| m.modified().ok());
        let size_bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
        let mut note = parse_note(path, &content);
        note.mtime = mtime;
        note.size_bytes = size_bytes;
        Ok(note)
    }
}

/// Build a [`Note`] from a path and its raw content.
///
/// Title is derived from the first H1 in the body, falling back to
/// the filename (without `.md`). Frontmatter, links, and tags are
/// extracted by [`crate::note_parser::parse`].
pub fn parse_note(path: &Path, content: &str) -> Note {
    let parsed = crate::note_parser::parse(content);
    let title = parsed
        .title
        .unwrap_or_else(|| title_from_path(path));
    Note {
        path: path.to_path_buf(),
        title,
        content: content.to_string(),
        frontmatter: parsed.frontmatter,
        links: parsed.links,
        tags: parsed.tags,
        mtime: None,
        size_bytes: 0,
    }
}

fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

/// Frontmatter as it appears in the markdown source, after the
/// leading `---`. Used by [`parse_note`] to read the YAML block
/// before handing the rest of the body to the link parser.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct FrontmatterAccess {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_from_h1() {
        let note = parse_note(
            Path::new("/v/note.md"),
            "# My Heading\n\nbody",
        );
        assert_eq!(note.title, "My Heading");
    }

    #[test]
    fn title_fallback_to_filename() {
        let note = parse_note(
            Path::new("/v/My Note.md"),
            "no heading here, just body",
        );
        assert_eq!(note.title, "My Note");
    }

    #[test]
    fn untitled_for_pathless() {
        let n = title_from_path(Path::new("/"));
        assert_eq!(n, "Untitled");
    }

    #[test]
    fn note_read_loads_file() {
        let dir = std::env::temp_dir().join(format!(
            "tungsten-note-read-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hello.md");
        std::fs::write(&path, "# Hello\n\nWorld\n").unwrap();
        let note = Note::read(&path).unwrap();
        assert_eq!(note.title, "Hello");
        assert!(note.content.contains("World"));
        assert!(note.size_bytes > 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn note_holds_frontmatter() {
        let src = "---\ntags: [a, b]\naliases: [A1, A2]\n---\n\n# x\n";
        let n = parse_note(Path::new("/v/x.md"), src);
        // frontmatter stays as serde_yaml::Value; check we can read tags
        if let serde_yaml::Value::Mapping(m) = &n.frontmatter {
            let tags = m.get(serde_yaml::Value::String("tags".into()));
            assert!(tags.is_some());
        } else {
            panic!("frontmatter should be a Mapping");
        }
        // tag list should include a, b
        assert!(n.tags.contains(&"a".to_string()));
        assert!(n.tags.contains(&"b".to_string()));
    }
}
