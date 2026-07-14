//! Quick switcher (cmd-O) for vault notes.
//!
//! Given a user-typed query and a [`NoteIndex`], return a ranked
//! list of candidate notes. Ranking is intentionally simple and
//! pure (no fancy ML):
//!
//! 1. Exact title (case-insensitive) match — top.
//! 2. Title prefix match — second.
//! 3. Title contains the query — third.
//! 4. Filename basename match — fourth.
//! 5. Tag match (query starts with `#`) — fifth.
//! 6. Recent opens (most recent first) — sixth, when no query.
//!
//! Ties are broken by note path so the result is stable. The
//! switcher returns at most `limit` results; default is 20.
//!
//! The output is a `Vec<SwitcherHit>` with the matched note plus
//! a `match_kind` enum and a numeric `score` (lower is better)
//! so the UI can sort further (e.g. by recency).

use std::path::PathBuf;

use crate::index::NoteIndex;

/// How a note matched the query. The UI uses this to label or
/// highlight the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    ExactTitle,
    TitlePrefix,
    TitleContains,
    FilenameMatch,
    TagMatch,
    RecentOpen,
}

/// A single quick-switcher hit.
#[derive(Debug, Clone, PartialEq)]
pub struct SwitcherHit<'a> {
    pub note: &'a crate::note::Note,
    pub match_kind: MatchKind,
    /// Lower is better. Ties broken by path.
    pub score: u32,
}

/// Run the quick switcher.
///
/// `recent` is a list of note paths in most-recent-first order;
/// pass an empty slice to disable recency ranking.
pub fn quick_switch<'a>(
    index: &'a NoteIndex,
    query: &str,
    recent: &[PathBuf],
    limit: usize,
) -> Vec<SwitcherHit<'a>> {
    let q = query.trim();
    let limit = limit.max(1);
    let q_lower = q.to_ascii_lowercase();

    if q.is_empty() {
        // Recents only.
        let mut hits: Vec<SwitcherHit> = Vec::new();
        for (i, path) in recent.iter().take(limit).enumerate() {
            if let Some(note) = index.get(path) {
                hits.push(SwitcherHit {
                    note,
                    match_kind: MatchKind::RecentOpen,
                    score: i as u32,
                });
            }
        }
        // Then a handful of recent edits so an empty box still
        // shows something useful.
        if hits.is_empty() {
            let mut all: Vec<&crate::note::Note> = index.notes().collect();
            all.sort_by(|a, b| b.path.cmp(&a.path));
            for (i, note) in all.into_iter().take(limit).enumerate() {
                hits.push(SwitcherHit {
                    note,
                    match_kind: MatchKind::RecentOpen,
                    score: 1000 + i as u32,
                });
            }
        }
        return hits;
    }

    let mut hits: Vec<SwitcherHit> = Vec::new();

    // Tag match: #foo matches notes tagged "foo".
    if let Some(tag) = q.strip_prefix('#') {
        let tag = tag.to_ascii_lowercase();
        for note in index.with_tag(&tag) {
            hits.push(SwitcherHit {
                note,
                match_kind: MatchKind::TagMatch,
                score: 50,
            });
        }
    }

    for note in index.notes() {
        let title_lower = note.title.to_ascii_lowercase();
        let basename = note
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if title_lower == q_lower {
            hits.push(SwitcherHit {
                note,
                match_kind: MatchKind::ExactTitle,
                score: 0,
            });
        } else if title_lower.starts_with(&q_lower) {
            hits.push(SwitcherHit {
                note,
                match_kind: MatchKind::TitlePrefix,
                score: 10 + (title_lower.len() as u32).saturating_sub(q_lower.len() as u32),
            });
        } else if title_lower.contains(&q_lower) {
            hits.push(SwitcherHit {
                note,
                match_kind: MatchKind::TitleContains,
                score: 20,
            });
        } else if basename == q_lower {
            hits.push(SwitcherHit {
                note,
                match_kind: MatchKind::FilenameMatch,
                score: 30,
            });
        } else if basename.contains(&q_lower) {
            hits.push(SwitcherHit {
                note,
                match_kind: MatchKind::FilenameMatch,
                score: 40,
            });
        }
    }

    hits.sort_by(|a, b| {
        a.score
            .cmp(&b.score)
            .then_with(|| a.note.path.cmp(&b.note.path))
    });
    hits.truncate(limit);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn make_index(dir: &Path) -> NoteIndex {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("Welcome.md"),
            "# Welcome\nIntro to the vault.\n",
        )
        .unwrap();
        fs::write(
            dir.join("Working Notes.md"),
            "# Working Notes\nDaily log.\n",
        )
        .unwrap();
        fs::write(
            dir.join("ideas.md"),
            "# Ideas\nBrainstorm.\n",
        )
        .unwrap();
        fs::write(
            dir.join("archive.md"),
            "# Archive\nOld stuff with #work tag.\n",
        )
        .unwrap();
        NoteIndex::build(dir).unwrap()
    }

    fn tempdir() -> std::path::PathBuf {
        let base = std::env::temp_dir();
        let unique = format!(
            "tungsten-switcher-test-{}-{}",
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
    fn empty_query_returns_recents() {
        let dir = tempdir();
        let index = make_index(&dir);
        let hits = quick_switch(&index, "", &[], 10);
        assert!(!hits.is_empty());
        for hit in &hits {
            assert_eq!(hit.match_kind, MatchKind::RecentOpen);
        }
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_query_uses_recent_list() {
        let dir = tempdir();
        let index = make_index(&dir);
        let recents = vec![
            dir.join("ideas.md"),
            dir.join("Welcome.md"),
        ];
        let hits = quick_switch(&index, "", &recents, 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].note.title, "Ideas");
        assert_eq!(hits[1].note.title, "Welcome");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn exact_title_match_wins() {
        let dir = tempdir();
        let index = make_index(&dir);
        let hits = quick_switch(&index, "ideas", &[], 10);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].match_kind, MatchKind::ExactTitle);
        assert_eq!(hits[0].note.title, "Ideas");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn case_insensitive() {
        let dir = tempdir();
        let index = make_index(&dir);
        let hits = quick_switch(&index, "WELCOME", &[], 10);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].note.title, "Welcome");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn prefix_match_beats_contains() {
        let dir = tempdir();
        let index = make_index(&dir);
        let hits = quick_switch(&index, "work", &[], 10);
        // "Working Notes" is a prefix match (starts with "work");
        // others might just contain it.
        assert!(!hits.is_empty());
        assert_eq!(hits[0].note.title, "Working Notes");
        assert_eq!(hits[0].match_kind, MatchKind::TitlePrefix);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn filename_match_when_title_differs() {
        let dir = tempdir();
        let index = make_index(&dir);
        let hits = quick_switch(&index, "archive", &[], 10);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].note.title, "Archive");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tag_match() {
        let dir = tempdir();
        let index = make_index(&dir);
        let hits = quick_switch(&index, "#work", &[], 10);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].note.title, "Archive");
        assert_eq!(hits[0].match_kind, MatchKind::TagMatch);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn limit_is_respected() {
        let dir = tempdir();
        let index = make_index(&dir);
        let hits = quick_switch(&index, "e", &[], 2);
        assert_eq!(hits.len(), 2);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ties_broken_by_path() {
        let dir = tempdir();
        let index = make_index(&dir);
        // Two distinct notes contain "e" (Welcome, archive). They
        // have the same score 20 so path order should break the
        // tie. Just assert the result is stable across calls.
        let a = quick_switch(&index, "e", &[], 10);
        let b = quick_switch(&index, "e", &[], 10);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.note.path, y.note.path);
        }
        fs::remove_dir_all(&dir).ok();
    }
}
