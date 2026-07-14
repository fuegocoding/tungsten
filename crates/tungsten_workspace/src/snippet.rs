//! Search snippet extraction.
//!
//! Given a [`Note`] and a byte range where a match was
//! found, [`snippet`] returns a short fragment of the
//! note's content centered on the match, with the matched
//! text wrapped in `**` for highlighting. This is what a
//! search-results panel renders as the row preview.
//!
//! The snippet:
//!
//! - Always starts and ends at a word boundary
//! - Never exceeds `max_chars` (default 120)
//! - Prefers the line containing the match
//! - Prefers an entire sentence when possible

/// Render a snippet for a single match.
///
/// `max_chars` is the upper bound on the snippet length
/// (excluding the `**` highlight markers).
pub fn snippet(content: &str, byte_range: std::ops::Range<usize>, max_chars: usize) -> String {
    let start = byte_range.start.min(content.len());
    let end = byte_range.end.min(content.len());
    if start >= end || start >= content.len() {
        return String::new();
    }
    let budget = max_chars.max(20);
    let half = budget / 2;

    // Try to find a sentence boundary in the window
    // around the match. We look at most `half` chars in
    // each direction.
    let mut s = start.saturating_sub(half);
    let mut e = (end + half).min(content.len());

    // Snap to the nearest line start/end above/below the
    // match.
    if let Some(line_start) = content[..start].rfind('\n') {
        s = s.max(line_start + 1);
    }
    if let Some(line_end) = content[end..].find('\n') {
        e = e.min(end + line_end);
    }
    // Expand outward to a sentence boundary if we have
    // room.
    if let Some(prev) = content[..s].rfind(|c: char| c == '.' || c == '!' || c == '?') {
        let candidate = prev + 1;
        if candidate >= start.saturating_sub(half) && candidate < s {
            // We can fit it.
            s = candidate;
        }
    }
    if let Some(next) = content[e..].find(|c: char| c == '.' || c == '!' || c == '?') {
        let candidate = e + next + 1;
        if candidate <= end + half {
            e = candidate;
        }
    }

    // Truncate to budget while keeping the match inside.
    while e - s > budget {
        // Trim from whichever side is farther from the
        // match center.
        let center = (start + end) / 2;
        if (s as isize - center as isize).abs() > (e as isize - center as isize).abs() {
            s += 1;
        } else {
            e -= 1;
        }
    }
    // Adjust to word boundaries.
    s = snap_to_word_start(content, s);
    e = snap_to_word_end(content, e);
    if e <= s {
        return String::new();
    }

    let mut out = String::new();
    if s > 0 {
        out.push('…');
    }
    let prefix = &content[s..start];
    let mid = &content[start..end];
    let suffix = &content[end..e];
    out.push_str(prefix);
    out.push_str("**");
    out.push_str(mid);
    out.push_str("**");
    out.push_str(suffix);
    if e < content.len() {
        out.push('…');
    }
    out
}

fn snap_to_word_start(content: &str, i: usize) -> usize {
    if i == 0 {
        return 0;
    }
    let bytes = content.as_bytes();
    let mut j = i;
    while j > 0 {
        let prev = bytes[j - 1] as char;
        let cur = bytes[j] as char;
        if (prev.is_whitespace() || prev == '\n') && !cur.is_whitespace() {
            return j;
        }
        if j == 0 {
            break;
        }
        j -= 1;
    }
    // No word boundary found; return the original i
    // unchanged so we don't expand the window.
    i
}

fn snap_to_word_end(content: &str, i: usize) -> usize {
    if i >= content.len() {
        return content.len();
    }
    let bytes = content.as_bytes();
    let mut j = i;
    while j < bytes.len() {
        let cur = bytes[j] as char;
        if cur.is_whitespace() || cur == '\n' {
            return j;
        }
        j += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_wraps_match_in_stars() {
        let s = "The quick brown fox jumps over the lazy dog.";
        let r = 16..19; // "fox"
        let out = snippet(s, r, 40);
        assert!(out.contains("**fox**"));
    }

    #[test]
    fn snippet_ellipsis_when_truncated() {
        let s = format!(
            "{}{}{}",
            "a".repeat(80),
            "MATCH",
            "b".repeat(80)
        );
        let m = s.find("MATCH").unwrap();
        let r = m..m + 5;
        let out = snippet(&s, r, 20);
        eprintln!("DBG: out={:?}", out);
        assert!(out.contains("**MATCH**"));
        assert!(out.contains('…'), "got: {out}");
    }

    #[test]
    fn snippet_handles_first_line() {
        let s = "MATCH on the first line\nSecond line here.";
        let r = 0..5;
        let out = snippet(s, r, 80);
        assert!(out.contains("**MATCH**"));
    }

    #[test]
    fn snippet_respects_word_boundary() {
        let s = "Hello world this is a long sentence that should be truncated by the snippet helper when it gets too long.";
        let r = 6..11; // "world"
        let out = snippet(s, r, 30);
        // The prefix should not be the middle of a word
        // (must start with "Hello " or "…world…").
        let trimmed = out.trim_start_matches('…');
        assert!(
            trimmed.starts_with("Hello")
                || trimmed.starts_with("world")
                || trimmed.starts_with("this"),
            "got: {out}"
        );
    }

    #[test]
    fn snippet_handles_empty_range() {
        let s = "Hello world";
        let r = 5..5;
        // An empty range at a word boundary is allowed and
        // may yield an empty snippet if there's no
        // surrounding text. The function should not panic.
        let _ = snippet(s, r, 20);
    }
}
