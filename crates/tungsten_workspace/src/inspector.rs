//! Vault inspector: a single-call diagnostic dump.
//!
//! Aggregates a [`NoteIndex`] into an [`InspectorReport`]
//! with the most useful stats for an at-a-glance view of
//! vault health: total notes, broken link count, orphan
//! count, largest notes, oldest by mtime, newest by mtime,
//! tags by frequency, and the top unresolved targets.
//!
//! The output is a stable JSON-serializable structure that
//! both the CLI (`tungsten-inspect`) and a future TUI
//! diagnostics panel can render.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::SystemTime;

use serde::Serialize;

use crate::index::NoteIndex;

/// One summary of vault health, computed on demand.
#[derive(Debug, Clone, Serialize)]
pub struct InspectorReport {
    pub note_count: usize,
    pub link_count: usize,
    pub tag_count: usize,
    pub orphan_count: usize,
    pub broken_link_count: usize,
    pub attachment_count: usize,
    pub total_size_bytes: u64,
    pub largest_notes: Vec<NoteSummary>,
    pub newest_notes: Vec<NoteSummary>,
    pub oldest_notes: Vec<NoteSummary>,
    pub top_tags: Vec<TagCount>,
    pub top_broken_targets: Vec<BrokenTarget>,
    pub folder_distribution: Vec<FolderCount>,
}

/// Lightweight per-note summary used inside the report.
#[derive(Debug, Clone, Serialize)]
pub struct NoteSummary {
    pub path: PathBuf,
    pub title: String,
    pub size_bytes: u64,
    pub mtime_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagCount {
    pub tag: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrokenTarget {
    pub target: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FolderCount {
    pub folder: String,
    pub count: usize,
}

/// Build the report.
///
/// `attachment_count` is provided by the caller because the
/// [`NoteIndex`] doesn't track attachments; the typical
/// caller will pass the result of
/// [`crate::attachments::AttachmentIndex::len`].
pub fn build_report(
    index: &NoteIndex,
    attachment_count: usize,
) -> InspectorReport {
    let mut total_size: u64 = 0;
    let mut notes: Vec<&crate::note::Note> = index.notes().collect();
    for n in &notes {
        total_size = total_size.saturating_add(n.size_bytes);
    }

    notes.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    let largest: Vec<NoteSummary> = notes
        .iter()
        .take(10)
        .map(|n| note_summary(n))
        .collect();

    let mut by_mtime: Vec<&crate::note::Note> =
        notes.iter().copied().collect();
    by_mtime.retain(|n| n.mtime.is_some());
    by_mtime.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    let newest: Vec<NoteSummary> = by_mtime
        .iter()
        .take(10)
        .map(|n| note_summary(n))
        .collect();
    let oldest: Vec<NoteSummary> = by_mtime
        .iter()
        .rev()
        .take(10)
        .map(|n| note_summary(n))
        .collect();

    let mut tag_counts: BTreeMap<String, usize> = BTreeMap::new();
    for n in &notes {
        for t in &n.tags {
            *tag_counts.entry(t.clone()).or_insert(0) += 1;
        }
    }
    let mut tag_vec: Vec<TagCount> = tag_counts
        .into_iter()
        .map(|(tag, count)| TagCount { tag, count })
        .collect();
    tag_vec.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));
    tag_vec.truncate(20);

    let mut broken_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut broken_total: usize = 0;
    for n in &notes {
        for link in &n.links {
            if index.by_title(&link.target).is_none() {
                *broken_counts.entry(link.target.clone()).or_insert(0) += 1;
                broken_total += 1;
            }
        }
    }
    let mut broken_vec: Vec<BrokenTarget> = broken_counts
        .into_iter()
        .map(|(target, count)| BrokenTarget { target, count })
        .collect();
    broken_vec.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.target.cmp(&b.target)));
    broken_vec.truncate(20);

    let mut folder_counts: BTreeMap<String, usize> = BTreeMap::new();
    for n in &notes {
        let folder = n
            .path
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();
        *folder_counts.entry(folder).or_insert(0) += 1;
    }
    let mut folder_vec: Vec<FolderCount> = folder_counts
        .into_iter()
        .map(|(folder, count)| FolderCount { folder, count })
        .collect();
    folder_vec.sort_by(|a, b| b.count.cmp(&a.count));
    folder_vec.truncate(20);

    InspectorReport {
        note_count: index.len(),
        link_count: index.stats().link_count,
        tag_count: index.stats().tag_count,
        orphan_count: index.orphans().len(),
        broken_link_count: broken_total,
        attachment_count,
        total_size_bytes: total_size,
        largest_notes: largest,
        newest_notes: newest,
        oldest_notes: oldest,
        top_tags: tag_vec,
        top_broken_targets: broken_vec,
        folder_distribution: folder_vec,
    }
}

fn note_summary(n: &crate::note::Note) -> NoteSummary {
    let mtime_secs = n
        .mtime
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    NoteSummary {
        path: n.path.clone(),
        title: n.title.clone(),
        size_bytes: n.size_bytes,
        mtime_secs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> std::path::PathBuf {
        let base = std::env::temp_dir();
        let unique = format!(
            "tungsten-inspect-test-{}-{}",
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
    fn report_counts_match_index() {
        let dir = tempdir();
        fs::create_dir_all(dir.join("subfolder")).unwrap();
        fs::write(dir.join("A.md"), "# A\nLinks to [[B]] and [[Missing]].\n").unwrap();
        fs::write(dir.join("B.md"), "# B\nBack to [[A]].\n").unwrap();
        fs::write(dir.join("subfolder/C.md"), "# C\nOrphan with #work tag.\n").unwrap();
        fs::write(dir.join("D.md"), "# D\n#work tag here.\n").unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let r = build_report(&index, 0);
        assert_eq!(r.note_count, 4);
        // B is referenced from A, so orphans = C and D? No, D has no incoming either, C has no incoming.
        // Actually C and D are both orphans.
        assert!(r.orphan_count >= 2);
        // Broken: A links to "Missing" (1 broken).
        assert_eq!(r.broken_link_count, 1);
        // Tags: #work appears in C and D.
        assert!(r.top_tags.iter().any(|t| t.tag == "work" && t.count == 2));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn report_empty_vault() {
        let dir = tempdir();
        let index = NoteIndex::build(&dir).unwrap();
        let r = build_report(&index, 0);
        assert_eq!(r.note_count, 0);
        assert_eq!(r.broken_link_count, 0);
        assert_eq!(r.orphan_count, 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn report_largest_notes_ordered() {
        let dir = tempdir();
        fs::write(dir.join("Small.md"), "# S\n").unwrap();
        fs::write(dir.join("Big.md"), format!("# B\n{}", "x".repeat(10_000))).unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let r = build_report(&index, 0);
        assert!(!r.largest_notes.is_empty());
        if r.largest_notes.len() >= 2 {
            assert!(r.largest_notes[0].size_bytes >= r.largest_notes[1].size_bytes);
        }
        fs::remove_dir_all(&dir).ok();
    }
}
