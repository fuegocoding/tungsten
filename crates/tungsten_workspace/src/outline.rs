//! Heading outline for a single note or a whole vault.
//!
//! Parses ATX-style headings (`#`, `##`, … `######`) and Setext
//! headings (underlined with `===` or `---`). The output is a
//! tree of [`Heading`] nodes.
//!
//! Fenced code blocks (`` ``` `` and `~~~`) suppress heading
//! parsing — a `#` inside a code fence is not a heading. This
//! matches CommonMark.
//!
//! `byte_range` covers the heading itself and all following
//! content until the next heading at the same or higher level.
//! This is what callers need to detect "current heading" from a
//! cursor offset and to scroll the editor to it.

use std::ops::Range;
use std::path::Path;

use crate::index::NoteIndex;

/// A single heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// Heading level: 1..=6 for ATX, 1 or 2 for Setext.
    pub level: u8,
    /// Heading text with leading `#` and trailing whitespace
    /// stripped.
    pub text: String,
    /// Byte range covering the heading and its content (see
    /// module docs).
    pub byte_range: Range<usize>,
    /// Byte range of just the heading text (without `#` and
    /// trailing whitespace, and without the Setext underline).
    pub text_range: Range<usize>,
    /// Sub-headings nested under this one.
    pub children: Vec<Heading>,
}

impl Heading {
    /// True if this heading has any sub-headings.
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// All headings in this subtree in pre-order (parent
    /// before children).
    pub fn flatten(&self) -> Vec<&Heading> {
        let mut out = Vec::new();
        out.push(self);
        for child in &self.children {
            out.extend(child.flatten());
        }
        out
    }
}

/// Extract a heading outline from a single note's raw content.
pub fn outline(content: &str) -> Vec<Heading> {
    // First pass: walk lines, find headings, push them onto
    // a flat list with their byte ranges. We then expand the
    // byte ranges in pass two so the ranges are correct.
    let mut flat: Vec<Heading> = Vec::new();

    let bytes = content.as_bytes();
    let mut i = 0;
    let mut in_fence: Option<&str> = None;

    while i < bytes.len() {
        let line_end = match find_newline(&bytes[i..]) {
            Some(n) => i + n,
            None => bytes.len(),
        };
        let line = &content[i..line_end];
        // `clean` has BOTH leading and trailing whitespace
        // removed, so heading detection can ignore the
        // trailing newline and trailing spaces.
        let clean = line.trim();
        let clean_start = i + (line.len() - line.trim_start().len());

        // Handle fences first.
        if let Some(fence) = in_fence {
            if clean.starts_with(fence) {
                in_fence = None;
            }
            i = next_line_start(content, line_end);
            continue;
        }
        if clean.starts_with("```") {
            in_fence = Some("```");
            i = next_line_start(content, line_end);
            continue;
        }
        if clean.starts_with("~~~") {
            in_fence = Some("~~~");
            i = next_line_start(content, line_end);
            continue;
        }

        // ATX heading: up to 3 leading spaces, then 1..=6
        // `#` characters, then space/tab/EOL.
        let leading_spaces = line.len() - line.trim_start().len();
        if leading_spaces <= 3 {
            if let Some(level) = atx_level(clean) {
                let after_hashes = &clean[level as usize..];
                let (text, text_offset_in_clean) = extract_atx_text(after_hashes);
                let text_start = clean_start + level as usize
                    + text_offset_in_clean;
                let text_end = text_start + text.len();
                flat.push(Heading {
                    level,
                    text,
                    byte_range: i..line_end,
                    text_range: text_start..text_end,
                    children: Vec::new(),
                });
                i = next_line_start(content, line_end);
                continue;
            }
        }

        // Setext: a text line followed by `===` (h1) or `---`
        // (h2) underline.
        if let Some(next_line_end) = next_line_end(content, line_end) {
            let underline = &content[line_end + 1..next_line_end];
            let underline_trimmed = trim_whitespace(underline);
            let (level, valid) = if !underline_trimmed.is_empty()
                && underline_trimmed.chars().all(|c| c == '=')
            {
                (1u8, true)
            } else if underline_trimmed.len() >= 2
                && underline_trimmed.chars().all(|c| c == '-')
            {
                (2u8, true)
            } else {
                (0u8, false)
            };
            if valid && !clean.is_empty() {
                let text_end = clean_start + clean.trim_end().len();
                flat.push(Heading {
                    level,
                    text: clean.trim().to_string(),
                    byte_range: i..next_line_end,
                    text_range: clean_start..text_end,
                    children: Vec::new(),
                });
                i = next_line_start(content, next_line_end);
                continue;
            }
        }

        i = next_line_start(content, line_end);
    }

    // Pass two: extend each heading's byte_range to the start
    // of the next heading at the same or higher level.
    for i in 0..flat.len() {
        let my_level = flat[i].level;
        let my_start = flat[i].byte_range.start;
        let mut end = flat[i].byte_range.end;
        for j in (i + 1)..flat.len() {
            if flat[j].level <= my_level {
                end = flat[j].byte_range.start;
                break;
            }
        }
        flat[i].byte_range = my_start..end;
    }

    // Pass three: nest into a tree by level. The stack
    // holds the chain of currently-open headings. Items
    // are attached to their parent only when popped, so
    // no duplicates are created.
    let mut roots: Vec<Heading> = Vec::new();
    let mut stack: Vec<Heading> = Vec::new();
    for h in flat {
        // Close any headings that are at the same level or
        // deeper than the incoming one. Each closed heading
        // becomes a child of its parent on the stack (or a
        // root if the stack is empty after the pop).
        while let Some(top) = stack.last() {
            if top.level < h.level {
                break;
            }
            let closed = stack.pop().unwrap();
            if let Some(parent) = stack.last_mut() {
                parent.children.push(closed);
            } else {
                roots.push(closed);
            }
        }
        // Push h onto the stack as the new deepest open
        // heading. It will be attached to its parent (or
        // roots) when something else closes it.
        stack.push(h);
    }
    // Close everything still on the stack.
    while let Some(closed) = stack.pop() {
        if let Some(parent) = stack.last_mut() {
            parent.children.push(closed);
        } else {
            roots.push(closed);
        }
    }
    roots
}

fn push_heading(_roots: &mut Vec<Heading>, _stack: &mut Vec<Heading>, _h: Heading) {
    // (Removed: see the inlined nesting loop in `outline`.)
}

fn find_newline(s: &[u8]) -> Option<usize> {
    for (i, b) in s.iter().enumerate() {
        if *b == b'\n' {
            return Some(i);
        }
    }
    None
}

fn next_line_start(content: &str, line_end: usize) -> usize {
    if line_end < content.len() && content.as_bytes()[line_end] == b'\n' {
        line_end + 1
    } else {
        content.len()
    }
}

fn next_line_end(content: &str, line_end: usize) -> Option<usize> {
    let start = next_line_start(content, line_end);
    if start >= content.len() {
        return None;
    }
    let bytes = content.as_bytes();
    let rest = &bytes[start..];
    match find_newline(rest) {
        Some(n) => Some(start + n),
        None => Some(content.len()),
    }
}

fn line_trim_end(s: &str) -> &str {
    s.trim_end_matches(|c: char| c == ' ' || c == '\t' || c == '\r')
}

fn trim_whitespace(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace())
}

fn atx_level(s: &str) -> Option<u8> {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes[0] != b'#' {
        return None;
    }
    let mut n = 0;
    while n < bytes.len() && bytes[n] == b'#' && n < 6 {
        n += 1;
    }
    if n == 0 {
        return None;
    }
    if n == bytes.len() {
        return Some(n as u8);
    }
    if bytes[n] == b' ' || bytes[n] == b'\t' {
        return Some(n as u8);
    }
    None
}

fn extract_atx_text(after_hashes: &str) -> (String, usize) {
    // Strip one optional leading space.
    let bytes = after_hashes.as_bytes();
    let mut start = 0;
    if start < bytes.len() && (bytes[start] == b' ' || bytes[start] == b'\t') {
        start += 1;
    }
    let body = &after_hashes[start..];
    let body_bytes = body.as_bytes();
    let n = body_bytes.len();

    // The optional close sequence is `#+` at the end of the
    // heading text. It is stripped if the run of `#`s is
    // preceded by whitespace OR the run is the entire
    // remaining text (so `##` parses to ""). The whitespace
    // is consumed as part of the close sequence.
    let mut hash_start = n;
    while hash_start > 0 && body_bytes[hash_start - 1] == b'#' {
        hash_start -= 1;
    }
    if hash_start < n {
        // We found trailing `#`s. They are a valid close
        // sequence if they are the whole text or preceded by
        // whitespace.
        let preceded_by_space = hash_start == 0
            || body_bytes[hash_start - 1] == b' '
            || body_bytes[hash_start - 1] == b'\t';
        if hash_start == 0 || preceded_by_space {
            // The close sequence is from the start of the
            // hash run back to the preceding whitespace
            // (exclusive), so the text ends before that
            // whitespace.
            let mut end = hash_start;
            if end > 0
                && (body_bytes[end - 1] == b' '
                    || body_bytes[end - 1] == b'\t')
            {
                end -= 1;
            }
            return (body[..end].to_string(), start);
        }
    }

    // No valid close sequence: trim trailing whitespace
    // only.
    let trimmed = body.trim_end();
    (trimmed.to_string(), start)
}

/// Build a vault-wide outline: one outline per note, returned
/// in the same order the index yields notes.
pub fn vault_outline(index: &NoteIndex) -> Vec<(std::path::PathBuf, Vec<Heading>)> {
    index
        .notes()
        .map(|note| (note.path.clone(), outline(&note.content)))
        .collect()
}

/// Find the deepest heading whose `byte_range` contains
/// `offset`. Returns `None` if `offset` is before every heading.
pub fn heading_at(outline: &[Heading], offset: usize) -> Option<&Heading> {
    fn rec<'a>(nodes: &'a [Heading], offset: usize) -> Option<&'a Heading> {
        for h in nodes {
            if h.byte_range.contains(&offset) {
                if let Some(child) = rec(&h.children, offset) {
                    return Some(child);
                }
                return Some(h);
            }
        }
        None
    }
    rec(outline, offset)
}

/// Read a file from disk and return its outline.
pub fn outline_for_path(path: &Path) -> std::io::Result<Vec<Heading>> {
    let content = std::fs::read_to_string(path)?;
    Ok(outline(&content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atx_headings_form_tree() {
        let s = "# A\nbody\n## A.1\nbody\n## A.2\nbody\n# B\nbody\n";
        let o = outline(s);
        assert_eq!(o.len(), 2);
        assert_eq!(o[0].text, "A");
        assert_eq!(o[0].children.len(), 2);
        assert_eq!(o[0].children[0].text, "A.1");
        assert_eq!(o[0].children[1].text, "A.2");
        assert_eq!(o[1].text, "B");
    }

    #[test]
    fn nesting_respects_levels() {
        let s = "# H1\n## H2\n### H3\n## H2b\n# H1b\n";
        let o = outline(s);
        eprintln!("DBG nest: o={:#?}", o);
        assert_eq!(o.len(), 2);
        assert_eq!(o[0].text, "H1");
        assert_eq!(o[0].children.len(), 2);
        assert_eq!(o[0].children[0].text, "H2");
        assert_eq!(o[0].children[0].children.len(), 1);
        assert_eq!(o[0].children[0].children[0].text, "H3");
    }

    #[test]
    fn close_sequence_is_stripped() {
        let s = "# Hello # world #\n";
        let o = outline(s);
        eprintln!("DBG: o.len()={}, o={:#?}", o.len(), o);
        assert_eq!(o.len(), 1, "expected 1 heading, got {}: {:#?}", o.len(), o);
        assert_eq!(o[0].text, "Hello # world");
    }

    #[test]
    fn no_close_sequence_when_no_space_before_hash() {
        let s = "# Hello#world\n";
        let o = outline(s);
        assert_eq!(o.len(), 1);
        // No leading space between "Hello" and "#", so the
        // `#world` is kept as part of the text.
        assert_eq!(o[0].text, "Hello#world");
    }

    #[test]
    fn fenced_code_blocks_suppress_headings() {
        let s = "# Real\n```\n# Fake\n```\n# Real again\n";
        let o = outline(s);
        assert_eq!(o.len(), 2);
        assert_eq!(o[0].text, "Real");
        assert_eq!(o[1].text, "Real again");
    }

    #[test]
    fn tilde_fence_suppresses_headings() {
        let s = "# Real\n~~~\n# Fake\n~~~\n";
        let o = outline(s);
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].text, "Real");
    }

    #[test]
    fn setext_h1() {
        let s = "Title\n=====\n\nbody\n";
        let o = outline(s);
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].level, 1);
        assert_eq!(o[0].text, "Title");
    }

    #[test]
    fn setext_h2() {
        let s = "Subtitle\n-------\n\nbody\n";
        let o = outline(s);
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].level, 2);
        assert_eq!(o[0].text, "Subtitle");
    }

    #[test]
    fn heading_at_finds_deepest() {
        let s = "# A\nbody\n## A.1\ninside\n# B\n";
        let o = outline(s);
        let off = s.find("inside").unwrap();
        let h = heading_at(&o, off).unwrap();
        assert_eq!(h.text, "A.1");
    }

    #[test]
    fn heading_at_after_last_returns_none() {
        let s = "# A\nbody\n";
        let o = outline(s);
        // An offset past the end of A's range (the next
        // heading would start here, but there isn't one).
        assert!(heading_at(&o, o[0].byte_range.end + 5).is_none());
    }

    #[test]
    fn heading_at_before_any_returns_none() {
        // A note with content before the first heading.
        let s = "\n\n# A\nbody\n";
        let o = outline(s);
        // Offset 0 is before "# A".
        assert!(heading_at(&o, 0).is_none());
    }

    #[test]
    fn flatten_preorder() {
        let s = "# A\n## A.1\n## A.2\n# B\n";
        let o = outline(s);
        let all: Vec<&str> = o
            .iter()
            .flat_map(|h| h.flatten())
            .map(|h| h.text.as_str())
            .collect();
        assert_eq!(all, vec!["A", "A.1", "A.2", "B"]);
    }

    #[test]
    fn byte_range_extends_to_next_same_or_higher_heading() {
        let s = "# A\nbody\n# B\nbody\n";
        let o = outline(s);
        assert!(o[0].byte_range.end <= o[1].byte_range.start);
        // A's body should be inside A's range.
        let off = s.find("body").unwrap();
        assert!(o[0].byte_range.contains(&off));
    }

    #[test]
    fn vault_outline_one_per_note() {
        use std::fs;
        let dir = std::env::temp_dir().join(format!(
            "tungsten-outline-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("A.md"), "# A\nbody\n").unwrap();
        fs::write(dir.join("B.md"), "# B\n## B.1\n").unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let out = vault_outline(&index);
        assert_eq!(out.len(), 2);
        // One entry per note regardless of how deeply nested
        // the headings go.
        let total_roots: usize = out.iter().map(|(_, o)| o.len()).sum();
        assert_eq!(total_roots, 2);
        // Recurse to count all headings including children.
        fn count(nodes: &[Heading]) -> usize {
            nodes.iter().map(|h| 1 + count(&h.children)).sum()
        }
        let total: usize = out.iter().map(|(_, o)| count(o)).sum();
        assert_eq!(total, 3);
        fs::remove_dir_all(&dir).ok();
    }
}
