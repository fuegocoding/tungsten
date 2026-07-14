//! Markdown parser for Tungsten notes.
//!
//! Two layers:
//! 1. A small YAML frontmatter scanner (extracts the leading `---`
//!    block and deserializes it).
//! 2. An Obsidian-extension scanner on the body (after frontmatter
//!    is stripped): wikilinks `[[...]]`, block refs `^id`, tags
//!    `#tag` (with path-segment awareness so we don't match `#`
//!    inside code spans or inside URL fragments).
//!
//! `pulldown-cmark` is used to identify "code-ish" regions (code
//! blocks + code spans) which are excluded from the Obsidian scan.
//! We don't use it to extract links because Obsidian wikilinks are
//! not part of CommonMark; trying to extend pulldown's link parser
//! is more work than just doing the regex pass with proper
//! boundaries.
//!
//! **What is *not* yet implemented:**
//! - Callout syntax (`> [!note]`) — the syntax is a single regex
//!   but extracting the body of a callout for inheritance is M2.3.
//! - Embeds (`![[Note]]`, `![[image.png|300x200]]`) — for v1, embeds
//!   appear in `links` as a `Wiki` link; the editor will render
//!   them differently in M1.1.
//! - Section links `[[Note##Section]]` — these are modeled but
//!   require walking the body for `## Heading` markers, which is M1.1.

use std::collections::BTreeSet;
use std::ops::Range;
use std::sync::LazyLock;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use regex::Regex;

use crate::note::{Link, LinkKind};

type LazyRegex = LazyLock<Regex>;

/// The output of parsing one note's body.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct ParsedNote {
    /// Title from the first H1, if any.
    pub title: Option<String>,
    /// YAML frontmatter as a deserialized `serde_yaml::Value`.
    pub frontmatter: serde_yaml::Value,
    /// Extracted links (wikilinks + markdown links).
    pub links: Vec<Link>,
    /// Extracted tags, normalized to lowercase, without the leading `#`.
    pub tags: Vec<String>,
    /// Callout blocks.
    pub callouts: Vec<crate::Callout>,
}

/// Parse a note's body into structured form.
pub(crate) fn parse(content: &str) -> ParsedNote {
    let (frontmatter_raw, body) = split_frontmatter(content);
    let frontmatter = parse_frontmatter(frontmatter_raw);
    let tags = extract_tags(&frontmatter, body);

    let code_ranges = collect_code_ranges(body);
    let links = extract_links(body, &code_ranges);

    // Title = first H1 in the body.
    let title = first_h1(body);

    // Callouts (M2.3): `> [!type] Title\n> body\n> more`.
    let callouts = extract_callouts(body);

    ParsedNote {
        title,
        frontmatter,
        links,
        tags,
        callouts,
    }
}

/// Strip the leading `---` YAML block, returning (frontmatter, body).
/// If there's no frontmatter, the entire content is the body.
fn split_frontmatter(content: &str) -> (&str, &str) {
    // Must start with `---\n` (or `---` followed by anything) to be
    // recognized as frontmatter. We require the closing `---` to be on
    // a line by itself.
    if !content.starts_with("---") {
        return ("", content);
    }
    let after_open = match content[3..].find('\n') {
        Some(n) => &content[3 + n + 1..],
        None => return ("", content),
    };
    // Find a line that is exactly `---` or `...`.
    let mut start = 0;
    for line in after_open.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" || trimmed == "..." {
            let body = &after_open[start + line.len()..];
            // Trim a single leading newline from the body.
            let body = body.strip_prefix('\n').unwrap_or(body);
            return (&after_open[..start], body);
        }
        start += line.len();
    }
    // No closing fence; treat the whole thing as body.
    ("", content)
}

fn parse_frontmatter(raw: &str) -> serde_yaml::Value {
    if raw.trim().is_empty() {
        return serde_yaml::Value::Null;
    }
    serde_yaml::from_str(raw).unwrap_or(serde_yaml::Value::Null)
}

/// Pull tags from the frontmatter's `tags:` key (list of strings) and
/// from inline `#tag` occurrences in the body. Dedupes, lowercases.
fn extract_tags(frontmatter: &serde_yaml::Value, body: &str) -> Vec<String> {
    let mut tags: BTreeSet<String> = BTreeSet::new();
    if let Some(fm_tags) = frontmatter.get("tags") {
        collect_yaml_string_list(fm_tags, &mut tags);
    }
    collect_inline_tags(body, &mut tags);
    tags.into_iter().collect()
}

fn collect_yaml_string_list(
    value: &serde_yaml::Value,
    out: &mut BTreeSet<String>,
) {
    match value {
        serde_yaml::Value::Sequence(seq) => {
            for v in seq {
                if let serde_yaml::Value::String(s) = v {
                    let normalized = s.trim().to_lowercase();
                    if !normalized.is_empty() {
                        out.insert(normalized);
                    }
                }
            }
        }
        serde_yaml::Value::String(s) => {
            for piece in s.split(',') {
                let normalized = piece.trim().to_lowercase();
                if !normalized.is_empty() {
                    out.insert(normalized);
                }
            }
        }
        _ => {}
    }
}

static INLINE_TAG_RE: LazyRegex = LazyLock::new(|| {
    // A `#tag` is `#` followed by a non-empty run of [a-z0-9_/-].
    // Word boundary on the left so we don't match mid-word (e.g. "hi#there"
    // — but "#tag" should match). We require either start-of-line,
    // whitespace, or one of `[(\[{` before the `#`.
    Regex::new(r"(?:^|[\s\(\[\{])#([a-z0-9][a-z0-9_/\-]*)").unwrap()
});

fn collect_inline_tags(body: &str, out: &mut BTreeSet<String>) {
    // Strip code regions to avoid false positives.
    let code_ranges = collect_code_ranges(body);
    for (start, caps) in INLINE_TAG_RE.captures_iter(body).enumerate() {
        // We need to skip captures inside code ranges. The `start` index
        // here is the capture index, not a byte offset; we re-derive the
        // byte offset from `caps.get(1).unwrap().start()` (the captured
        // group, not the full match).
        let Some(m) = caps.get(1) else { continue };
        let byte = m.start();
        if code_ranges.iter().any(|(s, e)| byte >= *s && byte < *e) {
            continue;
        }
        // The capture start is at `#`; we want the tag itself.
        // m.start() is the start of the captured group (i.e. right after #).
        // Use the full match for the leading char check.
        let _ = start;
        let normalized = m.as_str().to_lowercase();
        // Reject tags that are mostly digits and short, which are often
        // dates (e.g. #2024-01-01 is not a tag in Obsidian's sense).
        if normalized.chars().all(|c| c.is_ascii_digit() || c == '-') {
            continue;
        }
        if !normalized.is_empty() {
            out.insert(normalized);
        }
    }
}

/// Find all byte ranges that are inside code blocks or code spans
/// (so we can exclude them from link/tag scans). Uses pulldown-cmark
/// to walk the events.
fn collect_code_ranges(body: &str) -> Vec<(usize, usize)> {
    let parser = Parser::new_ext(body, Options::all());
    let mut ranges = Vec::new();
    let mut in_code_block: Option<(usize, usize)> = None;
    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = Some((range.start, range.end));
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((s, _)) = in_code_block.take() {
                    ranges.push((s, range.end));
                }
            }
            Event::Code(_) => {
                ranges.push((range.start, range.end));
            }
            _ => {}
        }
    }
    ranges
}

/// `[[Target]]`, `[[Target|alias]]`, `[[Target#Heading]]`,
/// `[[Target#^block]]`, `[[Target##Section]]`. This regex also
/// matches `![[...]]` embeds; we filter those out in
/// `extract_links` by checking whether the match is preceded by
/// `!`. (The `regex` crate doesn't support look-around.)
static WIKILINK_RE: LazyRegex = LazyLock::new(|| {
    Regex::new(r"\[\[([^\[\]]+?)\]\]").unwrap()
});

/// `![[Target]]` — Obsidian embed. Captures the inner target;
/// `extract_links` computes the `byte_range` to include the `!`.
static EMBED_RE: LazyRegex = LazyLock::new(|| {
    Regex::new(r"!\[\[([^\[\]]+?)\]\]").unwrap()
});

/// `[text](Target)` where Target is non-empty and contains no spaces.
/// We allow `path/note.md` style for completeness, but the editor
/// will treat `.md` extension specifically.
static MDLINK_RE: LazyRegex = LazyLock::new(|| {
    Regex::new(r#"(?m)\[([^\[\]]+)\]\(([^()\s]+)(?:\s+"[^"]*")?\)"#).unwrap()
});

fn extract_links(body: &str, code_ranges: &[(usize, usize)]) -> Vec<Link> {
    let mut links = Vec::new();

    // First, embeds. The byte range starts at `!` and ends at the
    // closing `]]`, so the editor knows the full extent to hide
    // on the focused line.
    for caps in EMBED_RE.captures_iter(body) {
        let full = caps.get(0).unwrap();
        let inner = caps.get(1).unwrap().as_str();
        let byte_range = full.start()..full.end();
        if code_ranges.iter().any(|(s, e)| byte_range.start >= *s && byte_range.start < *e) {
            continue;
        }
        let (raw_target, alias) = split_alias(inner);
        let _ = classify_wikilink(&raw_target);
        let target = strip_section_hash(&raw_target);
        links.push(Link {
            target,
            alias,
            kind: LinkKind::Embed,
            byte_range,
        });
    }

    for caps in WIKILINK_RE.captures_iter(body) {
        let full = caps.get(0).unwrap();
        let inner = caps.get(1).unwrap().as_str();
        let byte_range = full.start()..full.end();
        if code_ranges.iter().any(|(s, e)| byte_range.start >= *s && byte_range.start < *e) {
            continue;
        }
        // Filter out embeds: if the match is immediately preceded
        // by `!` (and not inside an `![` like an image with
        // brackets), the embed regex will catch it.
        if is_preceded_by_bang(body, byte_range.start) {
            continue;
        }
        let (raw_target, alias) = split_alias(inner);
        let kind = classify_wikilink(&raw_target);
        let target = strip_section_hash(&raw_target);
        links.push(Link {
            target,
            alias,
            kind,
            byte_range,
        });
    }

    for caps in MDLINK_RE.captures_iter(body) {
        let full = caps.get(0).unwrap();
        let target = caps.get(2).unwrap().as_str();
        let byte_range = full.start()..full.end();
        if code_ranges.iter().any(|(s, e)| byte_range.start >= *s && byte_range.start < *e) {
            continue;
        }
        if !is_markdown_link_target(target) {
            continue;
        }
        links.push(Link {
            target: target.to_string(),
            alias: None,
            kind: LinkKind::Markdown,
            byte_range,
        });
    }

    // Stable order: position in the document.
    links.sort_by_key(|l| l.byte_range.start);
    links
}

fn split_alias(inner: &str) -> (String, Option<String>) {
    // Obsidian splits on the FIRST `|`. Whitespace around the alias
    // is trimmed.
    if let Some(idx) = inner.find('|') {
        let raw = inner[..idx].to_string();
        let alias = inner[idx + 1..].trim().to_string();
        let alias = if alias.is_empty() { None } else { Some(alias) };
        (raw, alias)
    } else {
        (inner.to_string(), None)
    }
}

/// Returns true if the byte at `pos` in `body` is the second
/// character of a `![[...]]` embed (i.e. `pos - 1` is `!` and
/// `pos - 2` is not `!`).
fn is_preceded_by_bang(body: &str, pos: usize) -> bool {
    if pos == 0 || pos > body.len() {
        return false;
    }
    // The previous char must be `!`. We also want to make sure
    // the `!` is not part of `!!` (which is an image alt) — but
    // `!![[X]]` isn't standard, so a single `!` is fine.
    let prev = body.as_bytes().get(pos - 1).copied();
    prev == Some(b'!')
}

fn classify_wikilink(target: &str) -> LinkKind {
    // `[[#^block]]` is a self-block ref; not yet classified.
    if let Some(idx) = target.find("#^") {
        if idx == 0 {
            return LinkKind::WikiBlock;
        }
        return LinkKind::WikiBlock;
    }
    if let Some(idx) = target.find("##") {
        if idx == 0 {
            return LinkKind::WikiSection;
        }
        return LinkKind::WikiSection;
    }
    if target.contains('#') {
        return LinkKind::WikiHeading;
    }
    LinkKind::Wiki
}

/// Strip `#Heading` and `#^block` suffix from a target so that
/// `by_title` lookups can match on the note name alone. The full
/// target is still kept in [`Link::target`] (caller can use it for
/// cross-references).
fn strip_section_hash(target: &str) -> String {
    if let Some(idx) = target.find('#') {
        target[..idx].to_string()
    } else {
        target.to_string()
    }
}

fn is_markdown_link_target(target: &str) -> bool {
    if target.is_empty() {
        return false;
    }
    // External URLs are not "links to a note" — skip them. A target
    // is a note link if it ends in `.md` or has no scheme.
    if target.contains("://") {
        return false;
    }
    if target.starts_with('#') {
        return false; // in-page anchor
    }
    if target.starts_with("mailto:") || target.starts_with("tel:") {
        return false;
    }
    true
}

/// Extract the first H1 text from the body, if any.
fn first_h1(body: &str) -> Option<String> {
    let parser = Parser::new_ext(body, Options::all()).into_offset_iter();
    let mut in_h1 = false;
    let mut h1_start: Option<usize> = None;
    for (event, range) in parser {
        match event {
            Event::Start(Tag::Heading { level, .. })
                if level == pulldown_cmark::HeadingLevel::H1 =>
            {
                in_h1 = true;
                h1_start = Some(range.start);
            }
            Event::End(TagEnd::Heading(level)) if level == pulldown_cmark::HeadingLevel::H1 => {
                if let Some(start) = h1_start {
                    return Some(body[start..range.end].to_string());
                }
                in_h1 = false;
            }
            Event::Text(text) if in_h1 => {
                // First text inside an H1 — return an owned String so
                // it outlives the parser iterator.
                return Some(text.to_string());
            }
            _ => {}
        }
    }
    None
}

/// Extract callout blocks from the body. A callout is a sequence
/// of consecutive `>` lines, the first of which matches
/// `> [!type] Title` (the type is required, the title is
/// optional). A blank line ends the callout.
fn extract_callouts(body: &str) -> Vec<crate::Callout> {
    use std::sync::LazyLock;
    use regex::Regex;

    static CALLOUT_OPEN: LazyLock<Regex> = LazyLock::new(|| {
        // `> [!type]` or `> [!type] Title` at the start of a line.
        // `[^\s\[\]]+` for the type so we don't match `[!note]`-
        // like arrays; the type is a single token.
        Regex::new(r"^>\s*\[!([^\s\[\]]+)\]\s*(.*)$").unwrap()
    });
    static QUOTE_PREFIX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^>\s?(.*)$").unwrap());

    let mut out = Vec::new();
    let mut lines = body.split_inclusive('\n').peekable();
    while let Some(line) = lines.next() {
        // Strip the trailing newline for matching; we'll preserve
        // it in the byte range.
        let line_no_nl = line.trim_end_matches(['\r', '\n']);
        let Some(caps) = CALLOUT_OPEN.captures(line_no_nl) else {
            continue;
        };
        let kind = caps.get(1).unwrap().as_str().to_string();
        let title = {
            let t = caps.get(2).unwrap().as_str().trim();
            if t.is_empty() { None } else { Some(t.to_string()) }
        };
        let start = line_no_nl.as_ptr() as usize - body.as_ptr() as usize;
        let mut end = start + line_no_nl.len();
        // Consume continuation `> ` lines (with or without
        // content) until a non-`>` line or EOF.
        while let Some(next) = lines.peek() {
            let next_no_nl = next.trim_end_matches(['\r', '\n']);
            // A callout continuation starts with `>` (after
            // optional whitespace). A blank line ends the
            // callout.
            if QUOTE_PREFIX.is_match(next_no_nl) || next_no_nl.trim() == ">" {
                end = (next_no_nl.as_ptr() as usize - body.as_ptr() as usize)
                    + next_no_nl.len();
                lines.next();
            } else if next_no_nl.trim().is_empty() {
                // Blank line: end the callout here. We don't
                // consume the blank line.
                break;
            } else {
                break;
            }
        }
        out.push(crate::Callout {
            kind,
            title,
            byte_range: start..end,
        });
    }
    out
}

/// Sentinel for tests to keep the `Range` import live if all
/// in-file uses go away.
#[allow(dead_code)]
const _RANGE_USED: fn() -> Range<usize> = || 0..0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_frontmatter_means_empty_value() {
        let p = parse("# Title\n\nbody");
        assert!(matches!(p.frontmatter, serde_yaml::Value::Null));
        assert_eq!(p.title.as_deref(), Some("Title"));
    }

    #[test]
    fn frontmatter_parsed_as_yaml() {
        let p = parse("---\ntags: [a, b]\n---\n# T\n");
        let m = p.frontmatter.as_mapping().expect("mapping");
        assert!(m.contains_key("tags"));
    }

    #[test]
    fn frontmatter_unterminated_keeps_body_intact() {
        let p = parse("---\nnot: closed\n# Title\n");
        assert_eq!(p.title.as_deref(), Some("Title"));
    }

    #[test]
    fn wikilink_basic() {
        let p = parse("see [[Other Note]] for more");
        assert_eq!(p.links.len(), 1);
        assert_eq!(p.links[0].target, "Other Note");
        assert_eq!(p.links[0].kind, LinkKind::Wiki);
        assert_eq!(p.links[0].alias, None);
    }

    #[test]
    fn wikilink_with_alias() {
        let p = parse("see [[Other|the other one]] for more");
        assert_eq!(p.links[0].target, "Other");
        assert_eq!(p.links[0].alias.as_deref(), Some("the other one"));
    }

    #[test]
    fn wikilink_to_heading() {
        let p = parse("see [[Other#Section A]]");
        assert_eq!(p.links[0].target, "Other");
        assert_eq!(p.links[0].kind, LinkKind::WikiHeading);
    }

    #[test]
    fn wikilink_to_block() {
        let p = parse("see [[Other#^my-block]]");
        assert_eq!(p.links[0].target, "Other");
        assert_eq!(p.links[0].kind, LinkKind::WikiBlock);
    }

    #[test]
    fn wikilink_in_code_span_is_excluded() {
        let p = parse("don't link: `[[in code]]`");
        assert!(p.links.is_empty());
    }

    #[test]
    fn embed_basic_image() {
        let p = parse("see ![[photo.png]] for context");
        assert_eq!(p.links.len(), 1);
        assert_eq!(p.links[0].target, "photo.png");
        assert_eq!(p.links[0].kind, LinkKind::Embed);
    }

    #[test]
    fn embed_note_reference() {
        let p = parse("![[Note]]");
        assert_eq!(p.links.len(), 1);
        assert_eq!(p.links[0].target, "Note");
        assert_eq!(p.links[0].kind, LinkKind::Embed);
    }

    #[test]
    fn embed_with_alias_caption() {
        let p = parse("![[photo.png|a sunset photo]]");
        assert_eq!(p.links[0].target, "photo.png");
        assert_eq!(p.links[0].alias.as_deref(), Some("a sunset photo"));
        assert_eq!(p.links[0].kind, LinkKind::Embed);
    }

    #[test]
    fn embed_in_code_span_excluded() {
        let p = parse("not really: `![[image.png]]`");
        assert!(p.links.is_empty());
    }

    #[test]
    fn bang_alone_is_not_an_embed() {
        // `!` without `[[` should not match
        let p = parse("hello! world [[Note]]");
        assert_eq!(p.links.len(), 1);
        assert_eq!(p.links[0].kind, LinkKind::Wiki);
    }

    #[test]
    fn wikilink_in_fenced_code_excluded() {
        let p = parse("```\n[[not a link]]\n```\n");
        assert!(p.links.is_empty());
    }

    #[test]
    fn markdown_link_to_md_file() {
        let p = parse("see [the other](Other.md)");
        assert_eq!(p.links.len(), 1);
        assert_eq!(p.links[0].target, "Other.md");
        assert_eq!(p.links[0].kind, LinkKind::Markdown);
    }

    #[test]
    fn markdown_link_external_excluded() {
        let p = parse("[ext](https://example.com)");
        assert!(p.links.is_empty());
    }

    #[test]
    fn inline_tags_extracted() {
        let p = parse("text #alpha and #bravo/charlie more");
        assert!(p.tags.contains(&"alpha".to_string()));
        assert!(p.tags.contains(&"bravo/charlie".to_string()));
    }

    #[test]
    fn inline_tag_in_code_excluded() {
        let p = parse("`#not-a-tag`");
        assert!(p.tags.is_empty());
    }

    #[test]
    fn date_like_hash_not_a_tag() {
        let p = parse("on #2024-01-15 we did it");
        assert!(p.tags.is_empty());
    }

    #[test]
    fn tags_from_frontmatter() {
        let p = parse("---\ntags: [A, B]\n---\n");
        assert!(p.tags.contains(&"a".to_string()));
        assert!(p.tags.contains(&"b".to_string()));
    }

    #[test]
    fn tags_deduped_and_lowercased() {
        let p = parse("---\ntags: [A, a]\n---\n#a and #A");
        let count = p.tags.iter().filter(|t| *t == "a").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn title_first_h1() {
        let p = parse("intro\n\n# Real Title\n\nbody");
        assert_eq!(p.title.as_deref(), Some("Real Title"));
    }

    #[test]
    fn title_fallback_none_when_no_h1() {
        let p = parse("just body, no heading");
        assert!(p.title.is_none());
    }

    #[test]
    fn multiple_links_sorted_by_position() {
        let p = parse("[[B]] then [[A]]");
        assert_eq!(p.links[0].target, "B");
        assert_eq!(p.links[1].target, "A");
    }

    #[test]
    fn callout_simple_note() {
        let p = parse("> [!note]\n> Body text\n");
        assert_eq!(p.callouts.len(), 1);
        assert_eq!(p.callouts[0].kind, "note");
        assert_eq!(p.callouts[0].title, None);
    }

    #[test]
    fn callout_with_title() {
        let p = parse("> [!warning] Watch out\n> Details here\n");
        assert_eq!(p.callouts.len(), 1);
        assert_eq!(p.callouts[0].kind, "warning");
        assert_eq!(p.callouts[0].title.as_deref(), Some("Watch out"));
    }

    #[test]
    fn callout_continuation_lines() {
        let p = parse("> [!tip] Hint\n> First\n> Second\n> Third\n");
        assert_eq!(p.callouts.len(), 1);
        let body_bytes = p.callouts[0].byte_range.len();
        assert!(body_bytes > 0);
        assert_eq!(p.callouts[0].kind, "tip");
    }

    #[test]
    fn callout_ends_at_blank_line() {
        let body = "> [!note] A\n> in callout\n\nAfter\n";
        let p = parse(body);
        assert_eq!(p.callouts.len(), 1);
        let end = p.callouts[0].byte_range.end;
        let after = &body[end..];
        assert!(after.starts_with("\n\n") || after.starts_with('\n'));
    }

    #[test]
    fn callout_not_in_regular_quote() {
        let p = parse("> Just a quote\n> no callout syntax\n");
        assert!(p.callouts.is_empty());
    }

    #[test]
    fn multiple_callouts() {
        let body = "> [!note] A\n\n> [!tip] B\n> body\n";
        let p = parse(body);
        assert_eq!(p.callouts.len(), 2);
        assert_eq!(p.callouts[0].kind, "note");
        assert_eq!(p.callouts[1].kind, "tip");
    }
}
