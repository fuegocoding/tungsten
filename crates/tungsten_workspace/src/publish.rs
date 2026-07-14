//! Publish — markdown-to-HTML rendering with Obsidian extensions.
//!
//! A focused subset of the Obsidian Publish feature: given a
//! [`Note`] (and the vault root for resolving relative paths),
//! produce a self-contained HTML string ready to be written to
//! disk by a static-site generator.
//!
//! **What this does:**
//! - Renders markdown to HTML using `pulldown-cmark` (already a
//!   workspace dep)
//! - Post-processes the result to apply Obsidian conventions:
//!     * `[[Note]]`         -> `<a class="wikilink" href="Note.html">Note</a>`
//!     * `[[Note|alias]]`   -> `<a class="wikilink" href="Note.html">alias</a>`
//!     * `[[Note#Heading]]` -> `<a class="wikilink" href="Note.html#Heading">Note</a>`
//!     * `[[Note#^block]]`  -> `<a class="wikilink" href="Note.html#^block">Note</a>`
//!     * `![[image.png]]`   -> `<img class="embedded-image" src="image.png" />`
//!     * Frontmatter is rendered as a `<table class="frontmatter">`
//! - Returns a self-contained string (no external resources except
//!   for the image embeds, which are referenced by relative path)
//!
//! **What this does NOT do (yet):**
//! - Apply a theme (stylesheet link is emitted but the stylesheet
//!   itself is not provided)
//! - Render callouts (`> [!note]`) — that's M2.3 in the editor
//! - Math (MathJax) and mermaid (Mermaid) — handled by the editor
//! - Tags, backlinks panel, etc. — those are site-shell concerns
//! - Whole-site generation (links between notes, sitemap, RSS) —
//!   the caller iterates notes and writes per-note HTML files

use std::path::Path;

use pulldown_cmark::{html, Options, Parser};

use crate::note::Note;

/// Render a single note to a self-contained HTML fragment
/// (just the body — the caller is expected to wrap it in their
/// own document shell, or use [`render_full_page`] for a
/// ready-to-write file).
pub fn render_html(note: &Note) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    let mut body = String::with_capacity(note.content.len() + 256);
    html::push_html(&mut body, Parser::new_ext(&note.content, options));
    body = apply_obsidian_extensions(&body);
    body
}

/// Render a single note to a full HTML page (with a minimal
/// shell). Suitable for writing to disk as `Note.html` during a
/// static-site build.
pub fn render_full_page(note: &Note) -> String {
    let body = render_html(note);
    let fm_table = render_frontmatter_table(&note.frontmatter);
    let title = html_escape(&note.title);
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{title}</title>
<link rel="stylesheet" href="tungsten.css">
</head>
<body>
<article class="tungsten-note">
<h1 class="note-title">{title}</h1>
{fm_table}
<div class="note-body">
{body}
</div>
</article>
</body>
</html>
"#
    )
}

/// Render YAML frontmatter as an HTML table.
pub fn render_frontmatter_table(fm: &serde_yaml::Value) -> String {
    let serde_yaml::Value::Mapping(map) = fm else {
        return String::new();
    };
    if map.is_empty() {
        return String::new();
    }
    let mut out = String::from(r#"<table class="frontmatter">"#);
    for (k, v) in map {
        let key = match k {
            serde_yaml::Value::String(s) => html_escape(s),
            _ => html_escape(&serde_yaml::to_string(k).unwrap_or_default()),
        };
        let val = yaml_to_html(v);
        out.push_str(&format!("<tr><th>{key}</th><td>{val}</td></tr>"));
    }
    out.push_str("</table>");
    out
}

fn yaml_to_html(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::Null => String::from("<code>null</code>"),
        serde_yaml::Value::Bool(b) => format!("<code>{b}</code>"),
        serde_yaml::Value::Number(n) => format!("<code>{n}</code>"),
        serde_yaml::Value::String(s) => html_escape(s),
        serde_yaml::Value::Sequence(seq) => {
            let parts: Vec<String> = seq.iter().map(yaml_to_html).collect();
            format!(
                "<ul>{}</ul>",
                parts
                    .iter()
                    .map(|p| format!("<li>{p}</li>"))
                    .collect::<String>()
            )
        }
        _ => html_escape(&serde_yaml::to_string(v).unwrap_or_default()),
    }
}

/// Apply Obsidian-specific post-processing to pulldown-cmark's
/// HTML output. Specifically: wikilinks, embed images, and
/// block references. Operates on the string with simple regex
/// passes (the pulldown-cmark parser already handles markdown
/// links; we just need the Obsidian extensions it doesn't know
/// about).
fn apply_obsidian_extensions(html: &str) -> String {
    // Order matters: embeds must run before wikilinks (so
    // ![[img]] is matched first and the leftover [[img]] is
    // skipped). The regexes are written so that the embed
    // variant is not matched by the wikilink regex.
    use std::sync::LazyLock;
    use regex::Regex;

    static EMBED_RE: LazyLock<Regex> = LazyLock::new(|| {
        // ![[path]]  -- single-line, no nested brackets
        Regex::new(r"!\[\[([^\[\]]+?)\]\]").unwrap()
    });
    static WIKILINK_RE: LazyLock<Regex> = LazyLock::new(|| {
        // [[target|alias]] or [[target#heading]] or [[target]]
        // Negative lookbehind for ! is implicit because EMBED_RE
        // runs first and replaces ![[...]] in-place.
        Regex::new(r"\[\[([^\[\]]+?)\]\]").unwrap()
    });
    let s = EMBED_RE.replace_all(html, |caps: &regex::Captures| {
        let inner = &caps[1];
        let (target, _alias) = split_alias(inner);
        let target = strip_section_hash(&target);
        format!(
            r#"<img class="embedded-image" src="{target}" alt="{target}" />"#
        )
    });
    let s = WIKILINK_RE.replace_all(&s, |caps: &regex::Captures| {
        let inner = &caps[1];
        let (target, alias) = split_alias(inner);
        let (bare, anchor) = split_anchor(&target);
        let bare_html_path = format!("{bare}.html");
        let display = alias.unwrap_or(bare.clone());
        let href = match anchor {
            Some(a) if a.starts_with('^') => format!("{bare_html_path}#{a}"),
            Some(a) => format!("{bare_html_path}#{a}"),
            None => bare_html_path,
        };
        format!(
            r#"<a class="wikilink" href="{}">{}</a>"#,
            html_escape(&href),
            html_escape(&display),
        )
    });
    s.to_string()
}

fn split_alias(s: &str) -> (String, Option<String>) {
    if let Some(idx) = s.find('|') {
        (s[..idx].to_string(), Some(s[idx + 1..].to_string()))
    } else {
        (s.to_string(), None)
    }
}

fn strip_section_hash(s: &str) -> String {
    if let Some(idx) = s.find('#') {
        s[..idx].to_string()
    } else {
        s.to_string()
    }
}

fn split_anchor(s: &str) -> (String, Option<String>) {
    if let Some(idx) = s.find('#') {
        (s[..idx].to_string(), Some(s[idx + 1..].to_string()))
    } else {
        (s.to_string(), None)
    }
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Note;
    use crate::note_parser;

    fn note(content: &str) -> Note {
        // Construct a Note from a parsed ParsedNote. The path
        // doesn't matter for the renderer.
        let parsed = note_parser::parse(content);
        Note {
            path: std::path::PathBuf::from("/v/note.md"),
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
    fn renders_paragraph() {
        let n = note("Hello world.\n");
        let html = render_html(&n);
        assert!(html.contains("<p>Hello world."));
    }

    #[test]
    fn renders_heading() {
        let n = note("# Title\n\nbody\n");
        let html = render_html(&n);
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<p>body"));
    }

    #[test]
    fn renders_bold_italic_code() {
        let n = note("**bold** *italic* `code`\n");
        let html = render_html(&n);
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
        assert!(html.contains("<code>code</code>"));
    }

    #[test]
    fn renders_wikilink_basic() {
        let n = note("see [[Other Note]] for context\n");
        let html = render_html(&n);
        assert!(html.contains(r#"<a class="wikilink" href="Other Note.html">Other Note</a>"#));
    }

    #[test]
    fn renders_wikilink_with_alias() {
        let n = note("see [[Other|the other one]]\n");
        let html = render_html(&n);
        assert!(html.contains(r#"<a class="wikilink" href="Other.html">the other one</a>"#));
    }

    #[test]
    fn renders_wikilink_with_heading() {
        let n = note("see [[Other#Some Section]]\n");
        let html = render_html(&n);
        assert!(html.contains(r#"href="Other.html#Some Section""#));
        assert!(html.contains(">Other<"));
    }

    #[test]
    fn renders_embed_image() {
        let n = note("before ![[photo.png]] after\n");
        let html = render_html(&n);
        assert!(html.contains(r#"<img class="embedded-image" src="photo.png""#));
        // After replacing the embed, the inner [[photo.png]] must
        // not appear (the wikilink pass shouldn't double-process
        // it).
        assert!(!html.contains("[[photo.png]]"));
    }

    #[test]
    fn renders_standard_markdown_link() {
        let n = note("[click here](https://example.com)\n");
        let html = render_html(&n);
        assert!(html.contains(r#"<a href="https://example.com">click here</a>"#));
    }

    #[test]
    fn renders_task_list() {
        let n = note("- [x] done\n- [ ] todo\n");
        let html = render_html(&n);
        assert!(html.contains("type=\"checkbox\""));
        assert!(html.contains("checked"));
    }

    #[test]
    fn renders_full_page() {
        let n = note("# Title\n\nbody\n");
        let html = render_full_page(&n);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<title>Title</title>"));
        assert!(html.contains(r#"<link rel="stylesheet" href="tungsten.css">"#));
        assert!(html.contains("<h1>Title</h1>"));
    }

    #[test]
    fn frontmatter_renders_as_table() {
        let n = note("---\nstatus: draft\ntags: [a, b]\n---\nbody");
        let html = render_full_page(&n);
        assert!(html.contains(r#"<table class="frontmatter">"#));
        assert!(html.contains("<th>status</th>"));
        assert!(html.contains("<td>draft</td>"));
    }

    #[test]
    fn html_escapes_special_chars() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape(r#"he said "hi""#), "he said &quot;hi&quot;");
    }
}
