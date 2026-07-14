//! Live Preview — overlay decorations for the Markdown editor.
//!
//! PRD §2.4.3 / §5.3: the centerpiece of Obsidian's editing
//! experience. The editor renders the focused line as raw
//! markdown; every other line is rendered with the markdown
//! syntax hidden or replaced (bold text shown bold, not as
//! `**bold**`; wikilinks shown as the link text, not as
//! `[[Note]]`; etc.).
//!
//! This module is the library side: given a note's content, it
//! computes a list of *overlays* — byte ranges and the
//! transformation to apply at each. The editor uses these to
//! draw the right thing.
//!
//! **Scope:**
//! - Wikilink `[[Note]]` / `[[Note|alias]]` / `[[Note#Heading]]` —
//!   the `[[` and `]]` are hidden on unfocused lines; the inner
//!   text is shown as a link.
//! - Tag `#tag` (inline) — rendered as a pill on unfocused
//!   lines.
//! - Embed `![[image.png]]` — the `![[` and `]]` are hidden;
//!   the inner path is shown as the image or a placeholder.
//! - Markdown formatting markers (`**`, `*`, `~~`, backticks) are
//!   left to pulldown-cmark's own event stream (the editor can
//!   subscribe to that separately).
//!
//! **Out of scope:** callout syntax (`> [!note]`), footnotes
//! (M2.3).

use std::ops::Range;

use crate::note::{Link, LinkKind, Note};
use crate::note_parser;

/// The kind of decoration overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayKind {
    /// A wikilink. Hide the `[[` and `]]` brackets; style the
    /// inner text as a link.
    Wikilink,
    /// An embed. Hide the `![[` and `]]`; the inner content is
    /// either an image (show the image) or a note/file ref
    /// (show a placeholder).
    Embed,
    /// A wikilink to a block (target starts with `#^`).
    BlockRef,
    /// A wikilink to a heading (target contains `#`).
    HeadingRef,
    /// An inline tag (`#foo`). Style as a pill.
    Tag,
    /// A markdown link. Style as a link (the URL is the target).
    MarkdownLink,
}

/// One overlay: a byte range and what to do with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    pub kind: OverlayKind,
    /// Byte range in the source content. The full range is what
    /// the editor should hide (or restyle) when the line is not
    /// focused.
    pub range: Range<usize>,
    /// The "display" string the editor shows in place of the
    /// range (when not focused). For a wikilink, this is the
    /// alias (or the target if no alias). For a tag, this is the
    /// tag name (e.g. `tag`). For an embed, this is the inner
    /// path.
    pub display: String,
    /// The link's destination (target for wikilinks/embeds, URL
    /// for markdown links, tag name for tags). Used by the editor
    /// to wire click handlers.
    pub target: String,
}

/// Compute all overlays for a note's content. The result is
/// ordered by start byte; ranges are non-overlapping (with the
/// exception of nested embeds, where the embed is matched first
/// and the leftover `[[...]]` is not).
pub fn compute_overlays(note: &Note) -> Vec<Overlay> {
    let mut out: Vec<Overlay> = Vec::new();
    for link in &note.links {
        let overlay = match link.kind {
            LinkKind::Embed => Some(Overlay {
                kind: OverlayKind::Embed,
                range: link.byte_range.clone(),
                display: display_for_link(link),
                target: link.target.clone(),
            }),
            LinkKind::Wiki => Some(Overlay {
                kind: OverlayKind::Wikilink,
                range: link.byte_range.clone(),
                display: display_for_link(link),
                target: link.target.clone(),
            }),
            LinkKind::WikiHeading => Some(Overlay {
                kind: OverlayKind::HeadingRef,
                range: link.byte_range.clone(),
                display: display_for_link(link),
                target: link.target.clone(),
            }),
            LinkKind::WikiBlock => Some(Overlay {
                kind: OverlayKind::BlockRef,
                range: link.byte_range.clone(),
                display: display_for_link(link),
                target: link.target.clone(),
            }),
            LinkKind::WikiSection => Some(Overlay {
                kind: OverlayKind::HeadingRef,
                range: link.byte_range.clone(),
                display: display_for_link(link),
                target: link.target.clone(),
            }),
            LinkKind::Markdown => Some(Overlay {
                kind: OverlayKind::MarkdownLink,
                range: link.byte_range.clone(),
                display: display_for_link(link),
                target: link.target.clone(),
            }),
        };
        if let Some(o) = overlay {
            out.push(o);
        }
    }
    // Tags: walk the body for inline #tag occurrences. We do
    // this against the raw body rather than relying on
    // Note::tags because the body byte ranges are what the
    // editor needs.
    for tag_overlay in extract_inline_tags(&note.content) {
        out.push(tag_overlay);
    }
    out.sort_by_key(|o| o.range.start);
    out
}

fn display_for_link(link: &Link) -> String {
    if let Some(alias) = &link.alias {
        alias.clone()
    } else {
        link.target.clone()
    }
}

fn extract_inline_tags(content: &str) -> Vec<Overlay> {
    use std::sync::LazyLock;
    use regex::Regex;

    static TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:^|[\s\(\[\{])#([a-z0-9][a-z0-9_/\-]*)").unwrap()
    });

    // Compute code ranges so we skip tags inside code blocks /
    // code spans. The parser's collect_code_ranges is private,
    // so we re-run a simplified version here.
    let code_ranges = code_ranges_pub(content);

    let mut out = Vec::new();
    for caps in TAG_RE.captures_iter(content) {
        let Some(m) = caps.get(1) else { continue };
        let start = m.start();
        if code_ranges.iter().any(|(s, e)| start >= *s && start < *e) {
            continue;
        }
        let needle = m.as_str();
        // Date-like tags (#2024-01-01) are excluded.
        if needle.chars().all(|c| c.is_ascii_digit() || c == '-') {
            continue;
        }
        // m.start() points at the first char of the captured
        // group (the tag name), so the leading '#' is at
        // start - 1. The range covers '#tagname' inclusive.
        let hash_pos = start - 1;
        out.push(Overlay {
            kind: OverlayKind::Tag,
            range: hash_pos..hash_pos + 1 + needle.len(),
            display: format!("#{needle}"),
            target: needle.to_lowercase(),
        });
    }
    out
}

fn code_ranges_pub(body: &str) -> Vec<(usize, usize)> {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
    let parser = Parser::new_ext(body, Options::all()).into_offset_iter();
    let mut ranges = Vec::new();
    let mut in_block: Option<usize> = None;
    for (event, range) in parser {
        match event {
            Event::Start(Tag::CodeBlock(_)) => in_block = Some(range.start),
            Event::End(TagEnd::CodeBlock) => {
                if let Some(s) = in_block.take() {
                    ranges.push((s, range.end));
                }
            }
            Event::Code(_) => ranges.push((range.start, range.end)),
            _ => {}
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn note(content: &str) -> Note {
        let parsed = note_parser::parse(content);
        Note {
            path: PathBuf::from("/v/n.md"),
            title: parsed.title.unwrap_or_else(|| "Untitled".into()),
            content: content.to_string(),
            frontmatter: parsed.frontmatter,
            links: parsed.links,
            tags: parsed.tags,
            mtime: None,
            size_bytes: 0,
        }
    }

    #[test]
    fn wikilink_overlay_covers_brackets() {
        let n = note("see [[Other Note]] for more\n");
        let overlays = compute_overlays(&n);
        assert_eq!(overlays.len(), 1);
        let o = &overlays[0];
        assert_eq!(o.kind, OverlayKind::Wikilink);
        assert_eq!(o.range, 4..18); // the whole [[Other Note]]
        assert_eq!(o.target, "Other Note");
        assert_eq!(o.display, "Other Note");
    }

    #[test]
    fn wikilink_alias_shows_alias() {
        let n = note("see [[Other|the other one]]\n");
        let overlays = compute_overlays(&n);
        assert_eq!(overlays[0].display, "the other one");
    }

    #[test]
    fn embed_overlay_is_recognized() {
        let n = note("see ![[photo.png]] here\n");
        let overlays = compute_overlays(&n);
        assert_eq!(overlays[0].kind, OverlayKind::Embed);
    }

    #[test]
    fn tag_overlay_covers_hash_and_name() {
        let n = note("inline #rust here\n");
        let overlays = compute_overlays(&n);
        let tag = overlays
            .iter()
            .find(|o| o.kind == OverlayKind::Tag)
            .expect("tag overlay");
        assert_eq!(tag.target, "rust");
        assert_eq!(tag.range, 7..12); // "#rust" at byte 7
    }

    #[test]
    fn tag_inside_code_is_excluded() {
        let n = note("`#not-a-tag`\n");
        let overlays = compute_overlays(&n);
        assert!(overlays
            .iter()
            .all(|o| o.kind != OverlayKind::Tag),
            "tags in code should be excluded: {overlays:?}");
    }

    #[test]
    fn date_like_hash_is_excluded() {
        let n = note("on #2024-01-15 we did it\n");
        let overlays = compute_overlays(&n);
        assert!(overlays
            .iter()
            .all(|o| o.kind != OverlayKind::Tag),
            "date-like hashes should be excluded");
    }

    #[test]
    fn multiple_overlays_are_sorted_by_position() {
        let n = note("#alpha [[B]] #beta\n");
        let overlays = compute_overlays(&n);
        let positions: Vec<usize> = overlays.iter().map(|o| o.range.start).collect();
        let mut sorted = positions.clone();
        sorted.sort();
        assert_eq!(positions, sorted, "overlays should be in source order");
    }

    #[test]
    fn markdown_link_overlay() {
        // Use a local file (not an external URL) so the parser
        // records it as a note link.
        let n = note("see [the docs](note.md)\n");
        let overlays = compute_overlays(&n);
        assert!(overlays
            .iter()
            .any(|o| o.kind == OverlayKind::MarkdownLink));
    }

    #[test]
    fn heading_ref_kind() {
        let n = note("see [[Other#Section A]]\n");
        let overlays = compute_overlays(&n);
        assert_eq!(overlays[0].kind, OverlayKind::HeadingRef);
    }

    #[test]
    fn block_ref_kind() {
        let n = note("see [[Other#^my-block]]\n");
        let overlays = compute_overlays(&n);
        assert_eq!(overlays[0].kind, OverlayKind::BlockRef);
    }
}
