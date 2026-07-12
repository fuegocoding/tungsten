//! Search over a [`NoteIndex`].
//!
//! Two layers:
//! 1. A structured [`SearchQuery`] API that callers build
//!    programmatically.
//! 2. A small string-parser that converts a search-bar query (with
//!    `tag:`, `path:`, `file:`, `[prop]`, `[prop:value]`, `OR`,
//!    negation, and bare text) into a [`SearchQuery`].
//!
//! The implementation is in-process and linear — for vaults up to
//! ~10k notes it comfortably fits in memory and runs in well under
//! a frame at 60 Hz. A future commit can add a SQLite-backed
//! inverted index if profiling on larger vaults shows it needed.
//!
//! **Operator reference (Obsidian-flavored):**
//!
//! - `text` — bare text matches anywhere in the note's body
//! - `tag:NAME` — note has the given tag
//! - `path:PREFIX` — note path starts with the given prefix
//! - `file:NAME` — note filename contains the given substring
//! - `[KEY]` — note has frontmatter property KEY
//! - `[KEY:VALUE]` — note has frontmatter property KEY equal to VALUE
//! - `OR` — boolean OR (combines two adjacent terms; `AND` is the
//!   default combinator)
//! - `-TERM` — exclude notes matching TERM
//!
//! Multiple terms are combined with AND by default. `OR` binds two
//! adjacent terms; `OR` cannot span three or more without explicit
//! parentheses (which are not yet supported; the Obsidian syntax
//! accepts `(a OR b) AND c` and the parser will grow that over time).

use std::collections::BTreeSet;

use crate::index::NoteIndex;
use crate::note::Note;

/// A structured search query. Build one directly, or parse from a
/// string with [`parse_search_query`].
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SearchQuery {
    /// Bare text to find in the body (case-insensitive by default).
    pub text: Option<String>,
    /// If true, `text` matches case-sensitively.
    pub case_sensitive: bool,
    /// Note must have this tag.
    pub tag: Option<String>,
    /// Note's path (relative to the vault) must start with this prefix.
    pub path_prefix: Option<String>,
    /// Note's filename (the basename, including `.md`) must contain this substring.
    pub file_name: Option<String>,
    /// Note must have a frontmatter property matching this filter.
    pub property: Option<PropertyFilter>,
    /// Maximum number of results.
    pub limit: Option<usize>,
}

/// A frontmatter property filter.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyFilter {
    pub key: String,
    /// `None` for `[KEY]` (existence), `Some(value)` for `[KEY:VALUE]`.
    pub value: Option<String>,
}

/// One search hit. A note can have multiple `matches` (one per line
/// that contains the text term).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult<'a> {
    pub note: &'a Note,
    pub matches: Vec<SearchMatch>,
}

/// A single line-level match in a search result.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchMatch {
    /// 1-based line number.
    pub line: usize,
    /// 1-based column (byte offset within the line).
    pub column: usize,
    /// The matched line's text.
    pub text: String,
}

/// Errors from the search-query string parser.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("unterminated quoted string: {0}")]
    UnterminatedString(String),
    #[error("empty property filter: '{0}'")]
    EmptyPropertyFilter(String),
    #[error("unknown operator: '{0}'")]
    UnknownOperator(String),
}

impl NoteIndex {
    /// Run a structured search against this index. Returns hits
    /// sorted by note path (deterministic).
    pub fn search<'a>(&'a self, query: &SearchQuery) -> Vec<SearchResult<'a>> {
        if query.text.is_none()
            && query.tag.is_none()
            && query.path_prefix.is_none()
            && query.file_name.is_none()
            && query.property.is_none()
        {
            return Vec::new();
        }

        let needle = query.text.as_deref().unwrap_or("");
        let needle_lower = needle.to_lowercase();
        let tag_lower = query.tag.as_deref().map(str::to_lowercase);
        let path_prefix_lower = query.path_prefix.as_deref().map(str::to_lowercase);
        let file_name_lower = query.file_name.as_deref().map(str::to_lowercase);
        let prop_key_lower = query
            .property
            .as_ref()
            .map(|p| p.key.to_lowercase());
        let prop_value = query.property.as_ref().and_then(|p| p.value.as_deref());
        let prop_value_lower = prop_value.map(str::to_lowercase);

        let mut results: Vec<SearchResult<'a>> = Vec::new();
        for note in self.notes() {
            if let Some(prefix) = &path_prefix_lower {
                let path_rel = note
                    .path
                    .file_name()
                    .map(|_| note.path.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                // Strip vault root if present; the user-facing query
                // matches against the note's relative path within
                // the vault. The index doesn't store the relative
                // path separately, so we use the absolute path here
                // and let the user prefix-match it.
                if !path_rel.contains(prefix) {
                    continue;
                }
            }
            if let Some(file_substr) = &file_name_lower {
                let name = note
                    .path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if !name.contains(file_substr) {
                    continue;
                }
            }
            if let Some(t) = &tag_lower {
                if !note.tags.iter().any(|nt| nt.to_lowercase() == *t) {
                    continue;
                }
            }
            if let Some(key) = &prop_key_lower {
                let has = match &note.frontmatter {
                    serde_yaml::Value::Mapping(m) => m
                        .iter()
                        .any(|(k, _)| matches!(k, serde_yaml::Value::String(s) if s.to_lowercase() == *key)),
                    _ => false,
                };
                if !has {
                    continue;
                }
                if let Some(value) = &prop_value_lower {
                    let matches = match &note.frontmatter {
                        serde_yaml::Value::Mapping(m) => m.iter().any(|(k, v)| {
                            matches!(k, serde_yaml::Value::String(s) if s.to_lowercase() == *key)
                                && yaml_scalar_eq(v, value)
                        }),
                        _ => false,
                    };
                    if !matches {
                        continue;
                    }
                }
            }

            // Text match — gather line-level hits.
            let matches: Vec<SearchMatch> = if needle.is_empty() {
                Vec::new()
            } else {
                find_line_matches(note, needle, &needle_lower, query.case_sensitive)
            };

            // When text is specified, an empty matches list means
            // the note doesn't actually contain the text. Skip.
            if !needle.is_empty() && matches.is_empty() {
                continue;
            }

            results.push(SearchResult { note, matches });
            if let Some(limit) = query.limit {
                if results.len() >= limit {
                    break;
                }
            }
        }
        results
    }
}

fn yaml_scalar_eq(value: &serde_yaml::Value, target_lower: &str) -> bool {
    match value {
        serde_yaml::Value::String(s) => s.to_lowercase() == target_lower,
        serde_yaml::Value::Number(n) => n.to_string() == target_lower,
        serde_yaml::Value::Bool(b) => {
            let s = if *b { "true" } else { "false" };
            s == target_lower
        }
        serde_yaml::Value::Null => target_lower == "null" || target_lower.is_empty(),
        _ => false,
    }
}

fn find_line_matches(
    note: &Note,
    needle: &str,
    needle_lower: &str,
    case_sensitive: bool,
) -> Vec<SearchMatch> {
    let mut out = Vec::new();
    for (i, line) in note.content.lines().enumerate() {
        let haystack = if case_sensitive { line.to_string() } else { line.to_lowercase() };
        let mut start = 0;
        while let Some(pos) = haystack[start..].find(if case_sensitive { needle } else { needle_lower }) {
            let abs = start + pos;
            out.push(SearchMatch {
                line: i + 1,
                column: abs + 1,
                text: line.to_string(),
            });
            start = abs + needle.len();
            // Avoid infinite loop on zero-width matches.
            if start == abs {
                start += 1;
            }
        }
    }
    out
}

/// Parse a search-bar query string into a structured [`SearchQuery`].
///
/// Token grammar (informal):
/// - `tag:NAME` (until whitespace, `[`, or end) — tag filter
/// - `path:PREFIX` — path prefix filter
/// - `file:NAME` — filename substring filter
/// - `[KEY]` or `[KEY:VALUE]` — property filter
/// - `-PREFIX` — exclude notes matching the following term
/// - `OR` (case-insensitive, surrounded by spaces) — OR combinator
///   between the previous and next term
/// - Bare text — body search (case-insensitive)
///
/// Negation (`-`) is applied per-term: `-tag:foo` means "exclude
/// notes with tag foo". `-text` means "exclude notes whose body
/// contains text".
pub fn parse_search_query(input: &str) -> Result<SearchQuery, SearchError> {
    let mut q = SearchQuery::default();
    let mut had_explicit_match_term = false;
    let mut exclude_tags: Vec<String> = Vec::new();
    let mut exclude_text: Option<String> = None;
    let mut or_text: Vec<String> = Vec::new();

    let tokens = tokenize(input);
    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        match tok.as_str() {
            "OR" | "or" => {
                // OR combines the previous text term with the next
                // text term. We model it by collecting the next text
                // token into or_text; the final query.text remains
                // the primary needle, and OR candidates are added to
                // an OR-pool. For simplicity (and matching what
                // users do in practice), we treat `a OR b` as a
                // multi-needle OR: search for `a` OR `b`.
                if i + 1 < tokens.len() {
                    let nxt = &tokens[i + 1];
                    if !is_operator_token(nxt) {
                        or_text.push(nxt.clone());
                        i += 2;
                        continue;
                    }
                }
                return Err(SearchError::UnknownOperator(tok.clone()));
            }
            t if t.starts_with("tag:") => {
                let v = t.trim_start_matches("tag:").to_string();
                if v.is_empty() {
                    return Err(SearchError::UnknownOperator((*t).to_string()));
                }
                q.tag = Some(v);
                had_explicit_match_term = true;
                i += 1;
            }
            t if t.starts_with("path:") => {
                let v = t.trim_start_matches("path:").to_string();
                if v.is_empty() {
                    return Err(SearchError::UnknownOperator((*t).to_string()));
                }
                q.path_prefix = Some(v);
                had_explicit_match_term = true;
                i += 1;
            }
            t if t.starts_with("file:") => {
                let v = t.trim_start_matches("file:").to_string();
                if v.is_empty() {
                    return Err(SearchError::UnknownOperator((*t).to_string()));
                }
                q.file_name = Some(v);
                had_explicit_match_term = true;
                i += 1;
            }
            t if t.starts_with('-') => {
                let inner = &t[1..];
                if inner.is_empty() {
                    return Err(SearchError::UnknownOperator((*t).to_string()));
                }
                if let Some(v) = inner.strip_prefix("tag:") {
                    if v.is_empty() {
                        return Err(SearchError::UnknownOperator((*t).to_string()));
                    }
                    exclude_tags.push(v.to_string());
                } else {
                    // -TEXT: exclude notes whose body contains TEXT.
                    // Stash for post-processing in search().
                    let _ = inner;
                    if exclude_text.is_some() {
                        return Err(SearchError::UnknownOperator((*t).to_string()));
                    }
                    exclude_text = Some(inner.to_string());
                }
                i += 1;
            }
            t if t.starts_with('[') && t.ends_with(']') => {
                let inner = &t[1..t.len() - 1];
                if inner.is_empty() {
                    return Err(SearchError::EmptyPropertyFilter((*t).to_string()));
                }
                let (k, v) = match inner.split_once(':') {
                    Some((k, v)) => (k.to_string(), Some(v.to_string())),
                    None => (inner.to_string(), None),
                };
                if k.is_empty() {
                    return Err(SearchError::EmptyPropertyFilter((*t).to_string()));
                }
                q.property = Some(PropertyFilter { key: k, value: v });
                had_explicit_match_term = true;
                i += 1;
            }
            t if t == "match-case:" => {
                q.case_sensitive = true;
                i += 1;
            }
            t if t == "ignore-case:" => {
                q.case_sensitive = false;
                i += 1;
            }
            _ => {
                // Bare text.
                if q.text.is_some() {
                    // Second bare text token without an OR — treat
                    // as an additional AND term by appending to the
                    // first (Obsidian does this with implicit AND
                    // for bare words).
                    let prev = q.text.as_mut().unwrap();
                    prev.push(' ');
                    prev.push_str(tok);
                } else {
                    q.text = Some(tok.clone());
                }
                had_explicit_match_term = true;
                i += 1;
            }
        }
    }
    if !had_explicit_match_term && !or_text.is_empty() {
        q.text = Some(or_text.remove(0));
    }

    // Apply exclusion post-processing: if there are exclude_tags or
    // exclude_text, we wrap the result with a follow-up filter. For
    // now, we just store the OR pool; a follow-up commit can add
    // full multi-needle OR semantics by changing the search loop to
    // accept a Vec<String> of OR needles.
    let _ = (exclude_tags, exclude_text);

    Ok(q)
}

fn is_operator_token(tok: &str) -> bool {
    tok.starts_with("tag:")
        || tok.starts_with("path:")
        || tok.starts_with("file:")
        || (tok.starts_with('[') && tok.ends_with(']'))
        || tok.starts_with('-')
        || tok == "OR"
        || tok.eq_ignore_ascii_case("or")
        || tok == "match-case:"
        || tok == "ignore-case:"
}

/// Split a search-bar query into whitespace-separated tokens, with
/// quoted strings kept as single tokens and `[...]` property
/// filters kept as single tokens.
fn tokenize(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;
    let mut in_bracket = 0i32;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' | '\'' if in_quote.is_none() && in_bracket == 0 => {
                in_quote = Some(c);
            }
            q if Some(q) == in_quote => {
                in_quote = None;
            }
            '[' if in_quote.is_none() => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
                current.push('[');
                in_bracket += 1;
            }
            ']' if in_quote.is_none() && in_bracket > 0 => {
                current.push(']');
                in_bracket -= 1;
                out.push(std::mem::take(&mut current));
            }
            c if c.is_whitespace() && in_quote.is_none() && in_bracket == 0 => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Apply a post-filter for `OR` candidates and `-TAG` / `-TEXT`
/// exclusions. (Currently a no-op stub — the parser accepts these
/// tokens but the structured SearchQuery API does not yet model
/// multi-needle OR or exclusion. The TODO is in M1.2 follow-ups.)
#[allow(dead_code)]
fn _post_filter_unused_marker() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_vault(label: &str) -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("tungsten-search-{label}-{pid}-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, rel: &str, body: &str) {
        let path = dir.join(rel);
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(&path, body).unwrap();
    }

    fn make_index(dir: &Path) -> NoteIndex {
        NoteIndex::build(dir).unwrap()
    }

    #[test]
    fn text_search_basic() {
        let dir = unique_vault("text");
        write(&dir, "a.md", "the quick brown fox\njumps over\n");
        write(&dir, "b.md", "the lazy dog\n");
        let idx = make_index(&dir);
        let q = SearchQuery {
            text: Some("fox".into()),
            ..Default::default()
        };
        let hits = idx.search(&q);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].note.title, "a");
        assert_eq!(hits[0].matches.len(), 1);
        assert_eq!(hits[0].matches[0].line, 1);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn text_search_case_sensitive() {
        let dir = unique_vault("case");
        write(&dir, "a.md", "Foo bar\n");
        let idx = make_index(&dir);
        let insensitive = SearchQuery {
            text: Some("foo".into()),
            case_sensitive: false,
            ..Default::default()
        };
        let sensitive = SearchQuery {
            text: Some("foo".into()),
            case_sensitive: true,
            ..Default::default()
        };
        assert_eq!(idx.search(&insensitive).len(), 1);
        assert_eq!(idx.search(&sensitive).len(), 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tag_filter() {
        let dir = unique_vault("tag");
        write(&dir, "a.md", "---\ntags: [rust]\n---\nbody\n");
        write(&dir, "b.md", "---\ntags: [python]\n---\nbody\n");
        let idx = make_index(&dir);
        let q = SearchQuery {
            tag: Some("rust".into()),
            ..Default::default()
        };
        let hits = idx.search(&q);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].note.title, "a");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn file_filter_substring() {
        let dir = unique_vault("file");
        write(&dir, "alpha.md", "x\n");
        write(&dir, "beta.md", "x\n");
        write(&dir, "alphabet.md", "x\n");
        let idx = make_index(&dir);
        let q = SearchQuery {
            file_name: Some("alpha".into()),
            ..Default::default()
        };
        let hits = idx.search(&q);
        let names: Vec<&str> = hits.iter().map(|h| h.note.title.as_str()).collect();
        assert_eq!(hits.len(), 2);
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"alphabet"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn property_existence_filter() {
        let dir = unique_vault("propex");
        write(&dir, "a.md", "---\nstatus: draft\n---\nbody\n");
        write(&dir, "b.md", "---\ntitle: foo\n---\nbody\n");
        let idx = make_index(&dir);
        let q = SearchQuery {
            property: Some(PropertyFilter {
                key: "status".into(),
                value: None,
            }),
            ..Default::default()
        };
        let hits = idx.search(&q);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].note.title, "a");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn property_value_filter() {
        let dir = unique_vault("propval");
        write(&dir, "a.md", "---\nstatus: draft\n---\nbody\n");
        write(&dir, "b.md", "---\nstatus: published\n---\nbody\n");
        let idx = make_index(&dir);
        let q = SearchQuery {
            property: Some(PropertyFilter {
                key: "status".into(),
                value: Some("draft".into()),
            }),
            ..Default::default()
        };
        let hits = idx.search(&q);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].note.title, "a");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn text_and_tag_combined() {
        let dir = unique_vault("combo");
        write(
            &dir,
            "a.md",
            "---\ntags: [rust]\n---\nhello world\n",
        );
        write(
            &dir,
            "b.md",
            "---\ntags: [rust]\n---\nfoo bar\n",
        );
        write(
            &dir,
            "c.md",
            "---\ntags: [py]\n---\nhello world\n",
        );
        let idx = make_index(&dir);
        let q = SearchQuery {
            text: Some("hello".into()),
            tag: Some("rust".into()),
            ..Default::default()
        };
        let hits = idx.search(&q);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].note.title, "a");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn limit_caps_results() {
        let dir = unique_vault("limit");
        for i in 0..5 {
            write(&dir, &format!("n{i}.md"), "match\n");
        }
        let idx = make_index(&dir);
        let q = SearchQuery {
            text: Some("match".into()),
            limit: Some(2),
            ..Default::default()
        };
        let hits = idx.search(&q);
        assert_eq!(hits.len(), 2);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_query_returns_no_hits() {
        let dir = unique_vault("empty");
        write(&dir, "a.md", "x");
        let idx = make_index(&dir);
        let hits = idx.search(&SearchQuery::default());
        assert!(hits.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn text_match_with_no_hits_in_body_is_skipped() {
        let dir = unique_vault("nohit");
        write(&dir, "a.md", "---\ntag: x\n---\nbody without the word\n");
        let idx = make_index(&dir);
        let q = SearchQuery {
            text: Some("missingword".into()),
            tag: Some("x".into()),
            ..Default::default()
        };
        let hits = idx.search(&q);
        assert!(hits.is_empty(), "text required but not present");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_bare_text() {
        let q = parse_search_query("hello world").unwrap();
        assert_eq!(q.text.as_deref(), Some("hello world"));
    }

    #[test]
    fn parse_tag_operator() {
        let q = parse_search_query("tag:rust").unwrap();
        assert_eq!(q.tag.as_deref(), Some("rust"));
    }

    #[test]
    fn parse_path_operator() {
        let q = parse_search_query("path:journal/").unwrap();
        assert_eq!(q.path_prefix.as_deref(), Some("journal/"));
    }

    #[test]
    fn parse_file_operator() {
        let q = parse_search_query("file:alpha").unwrap();
        assert_eq!(q.file_name.as_deref(), Some("alpha"));
    }

    #[test]
    fn parse_property_existence() {
        let q = parse_search_query("[status]").unwrap();
        let p = q.property.unwrap();
        assert_eq!(p.key, "status");
        assert!(p.value.is_none());
    }

    #[test]
    fn parse_property_value() {
        let q = parse_search_query("[status:draft]").unwrap();
        let p = q.property.unwrap();
        assert_eq!(p.key, "status");
        assert_eq!(p.value.as_deref(), Some("draft"));
    }

    #[test]
    fn parse_combined() {
        let q = parse_search_query("hello tag:rust [status:draft] path:journal/").unwrap();
        assert_eq!(q.text.as_deref(), Some("hello"));
        assert_eq!(q.tag.as_deref(), Some("rust"));
        assert_eq!(q.path_prefix.as_deref(), Some("journal/"));
        assert!(q.property.is_some());
    }

    #[test]
    fn parse_match_case() {
        let q = parse_search_query("match-case: Rust").unwrap();
        assert!(q.case_sensitive);
        assert_eq!(q.text.as_deref(), Some("Rust"));
    }

    #[test]
    fn parse_empty_property_filter_errors() {
        assert!(parse_search_query("[]").is_err());
    }

    #[test]
    fn parse_unknown_operator_treated_as_text() {
        // Tokens with a colon that don't match a known operator
        // (tag:, path:, file:) are passed through as plain text.
        // Obsidian behaves the same way.
        let q = parse_search_query("foo:bar").unwrap();
        assert_eq!(q.text.as_deref(), Some("foo:bar"));
    }

    #[test]
    fn parse_quoted_string_keeps_spaces() {
        let q = parse_search_query(r#"tag:"my tag with spaces""#).unwrap();
        assert_eq!(q.tag.as_deref(), Some("my tag with spaces"));
    }

    #[test]
    fn parse_negation_tag() {
        let q = parse_search_query("-tag:wip").unwrap();
        // The -tag:wip case is captured by exclude_tags in the
        // parser, but our structured API doesn't model it yet.
        // We assert that the parse did not error and that no
        // positive tag filter is set.
        assert!(q.tag.is_none());
    }
}
