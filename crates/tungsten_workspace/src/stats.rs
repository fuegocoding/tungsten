//! Vault statistics: word counts, reading time, longest
//! notes.
//!
//! Computes simple author-friendly statistics across a
//! vault: total word count, total reading time (assumed
//! 220 WPM), the longest notes by words, and the top
//! contributors by tag.
//!
//! All numbers are derived from the [`NoteIndex`] without
//! re-reading the file system.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::index::NoteIndex;

/// Per-vault statistics.
#[derive(Debug, Clone, Serialize)]
pub struct VaultStats {
    pub total_notes: usize,
    pub total_words: usize,
    pub total_chars: usize,
    pub total_chars_no_whitespace: usize,
    /// Estimated reading time at 220 WPM, in minutes.
    pub reading_time_minutes: u32,
    pub longest_notes: Vec<NoteStat>,
    pub top_tags: Vec<StatsTagCount>,
    pub word_histogram: WordHistogram,
}

/// Stats for a single note.
#[derive(Debug, Clone, Serialize)]
pub struct NoteStat {
    pub path: PathBuf,
    pub title: String,
    pub words: usize,
    pub chars: usize,
}

/// Tag with its total word count across all tagged notes.
#[derive(Debug, Clone, Serialize)]
pub struct StatsTagCount {
    pub tag: String,
    pub notes: usize,
    pub words: usize,
}

/// A coarse word-length histogram (5-char buckets).
#[derive(Debug, Clone, Serialize, Default)]
pub struct WordHistogram {
    pub short: usize,   // 1..=4 chars
    pub medium: usize,  // 5..=8 chars
    pub long: usize,    // 9..=12 chars
    pub extra_long: usize, // 13+ chars
}

/// Compute the statistics for a vault.
pub fn compute(index: &NoteIndex, wpm: u32) -> VaultStats {
    let mut total_words = 0;
    let mut total_chars = 0;
    let mut total_chars_no_ws = 0;
    let mut notes_data: Vec<NoteStat> = Vec::new();
    let mut tag_words: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut histogram = WordHistogram::default();

    for note in index.notes() {
        let (words, chars, chars_no_ws) = word_count(&note.content);
        total_words += words;
        total_chars += chars;
        total_chars_no_ws += chars_no_ws;
        notes_data.push(NoteStat {
            path: note.path.clone(),
            title: note.title.clone(),
            words,
            chars,
        });
        for w in note.content.split_whitespace() {
            let len = w.chars().count();
            match len {
                1..=4 => histogram.short += 1,
                5..=8 => histogram.medium += 1,
                9..=12 => histogram.long += 1,
                _ => histogram.extra_long += 1,
            }
        }
        for tag in &note.tags {
            let entry = tag_words.entry(tag.clone()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += words;
        }
    }

    notes_data.sort_by(|a, b| b.words.cmp(&a.words));
    let longest = notes_data.iter().take(10).cloned().collect();
    let top_tags: Vec<StatsTagCount> = tag_words
        .into_iter()
        .map(|(tag, (notes, words))| StatsTagCount { tag, notes, words })
        .collect();
    let reading_time = if wpm == 0 {
        0
    } else {
        (total_words as u32).div_ceil(wpm)
    };

    VaultStats {
        total_notes: index.len(),
        total_words,
        total_chars,
        total_chars_no_whitespace: chars_no_ws(total_chars, total_chars_no_ws),
        reading_time_minutes: reading_time,
        longest_notes: longest,
        top_tags,
        word_histogram: histogram,
    }
}

fn word_count(content: &str) -> (usize, usize, usize) {
    let words = content.split_whitespace().filter(|w| !w.is_empty()).count();
    let chars = content.chars().count();
    let chars_no_ws = content.chars().filter(|c| !c.is_whitespace()).count();
    (words, chars, chars_no_ws)
}

fn chars_no_ws(_total: usize, no_ws: usize) -> usize {
    no_ws
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> std::path::PathBuf {
        let base = std::env::temp_dir();
        let unique = format!(
            "tungsten-stats-test-{}-{}",
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
    fn word_count_basic() {
        let (w, c, cws) = word_count("Hello world this is five.");
        assert_eq!(w, 5);
        assert!(c >= 25);
        assert!(cws < c);
    }

    #[test]
    fn stats_aggregate_words() {
        let dir = tempdir();
        // Bodies only (no headings) so the test is robust
        // to whether the parser includes the `# Title` as
        // tokens.
        fs::write(dir.join("A.md"), "one two three four\n").unwrap();
        fs::write(dir.join("B.md"), "five six\n").unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let s = compute(&index, 220);
        assert_eq!(s.total_notes, 2);
        assert_eq!(s.total_words, 6);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reading_time_uses_wpm() {
        let dir = tempdir();
        // 220 words → exactly 1 minute at 220 WPM.
        let content = "word ".repeat(220);
        fs::write(dir.join("A.md"), content).unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let s = compute(&index, 220);
        assert_eq!(s.reading_time_minutes, 1);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn longest_notes_are_ordered() {
        let dir = tempdir();
        fs::write(dir.join("Short.md"), "# S\none\n").unwrap();
        fs::write(dir.join("Long.md"), format!("# L\n{}", "x ".repeat(500))).unwrap();
        fs::write(dir.join("Mid.md"), format!("# M\n{}", "y ".repeat(50))).unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let s = compute(&index, 220);
        assert_eq!(s.longest_notes[0].title, "L");
        assert_eq!(s.longest_notes[1].title, "M");
        assert_eq!(s.longest_notes[2].title, "S");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn histogram_buckets_words() {
        let dir = tempdir();
        // 1, 12, 22 char words
        fs::write(dir.join("A.md"), "a xxxxxxxxxxxx yyyyyyyyyyyyyyyyyyyyyy\n").unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let s = compute(&index, 220);
        assert_eq!(s.word_histogram.short, 1);
        assert_eq!(s.word_histogram.long, 1);
        assert_eq!(s.word_histogram.extra_long, 1);
        fs::remove_dir_all(&dir).ok();
    }
}
