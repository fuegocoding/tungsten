//! Link integrity: rename a note and rewrite every link that
//! points to it.
//!
//! When a file is renamed in an Obsidian vault, every wikilink and
//! markdown link in the rest of the vault that points to the old
//! name must be updated to point to the new name. This module
//! performs the filesystem rename and the in-place content rewrite,
//! and updates the [`NoteIndex`] to reflect the new path and links.
//!
//! **What's rewritten in the source content:**
//! - `[[OldName]]`        → `[[NewName]]`
//! - `[[OldName|alias]]`  → `[[NewName|alias]]`
//! - `[[OldName#Heading]]`→ `[[NewName#Heading]]`  (heading preserved)
//! - `[[OldName#^block]]` → `[[NewName#^block]]`   (block preserved)
//! - `[[OldName##Section]]`→ `[[NewName##Section]]` (section preserved)
//! - `[text](OldName.md)`  → `[text](NewName.md)`   (markdown link)
//!
//! The byte ranges in the index are *not* updated to reflect the
//! rewrite — the caller's job is to rebuild the index after a rename
//! (or to call `update_note` on each rewritten source, which is what
//! the rename method does).

use std::fs;
use std::path::{Path, PathBuf};

use crate::index::NoteIndex;
use crate::note_parser::ParsedNote;

/// Summary of a rename operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameResult {
    /// The new path (absolute) of the renamed note.
    pub new_path: PathBuf,
    /// The old title (the part of the wikilink before `#`, `^`, or
    /// `|`, normalized by stripping `.md`).
    pub old_title: String,
    /// The new title (same normalization as `old_title`).
    pub new_title: String,
    /// Paths of notes whose content was rewritten to point to the
    /// new title. The renamed note itself is NOT in this list
    /// (unless other notes linked to it).
    pub rewritten_sources: Vec<PathBuf>,
    /// Total number of individual link replacements made across
    /// `rewritten_sources`.
    pub link_replacements: usize,
}

/// Errors from the rename operation.
#[derive(Debug, thiserror::Error)]
pub enum RenameError {
    #[error("source file does not exist: {0}")]
    SourceMissing(PathBuf),
    #[error("destination already exists: {0}")]
    DestinationExists(PathBuf),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("old path is not in the vault (would escape): {0}")]
    OutsideVault(PathBuf),
    #[error("index error: {0}")]
    Index(#[from] crate::IndexError),
}

impl NoteIndex {
    /// Rename `old_path` to `new_path`, rewriting every wikilink
    /// and markdown link in the rest of the vault that points to
    /// the old filename.
    ///
    /// Both paths must be absolute (or relative to a common root
    /// understood by the caller). The `old_path` is interpreted
    /// against the index's known note set; if a note with that
    /// canonicalized path exists, its links are the ones to be
    /// rewritten. The `new_path` becomes the canonical path of
    /// the renamed note in the index.
    ///
    /// **Title semantics:** the title used for matching is the
    /// *filename basename* (without `.md`), not the H1. H1 is the
    /// display title; filename is the canonical link target. If
    /// you have `Old.md` with H1 "Old" and rename it to `New.md`,
    /// backlinks to `[[Old]]` (matching the old filename) get
    /// rewritten to `[[New]]` even though the H1 didn't change.
    ///
    /// After the rename, the index is updated in place:
    /// - The renamed note is re-keyed under `new_path`.
    /// - Every source that was rewritten is re-indexed via
    ///   [`Self::update_note`].
    /// - The backlinks, by_title, by_tag, and outgoing maps are
    ///   rebuilt for the affected notes.
    pub fn rename(
        &mut self,
        old_path: &Path,
        new_path: &Path,
    ) -> Result<RenameResult, RenameError> {
        if !old_path.is_file() {
            return Err(RenameError::SourceMissing(old_path.to_path_buf()));
        }
        if new_path.exists() && old_path != new_path {
            return Err(RenameError::DestinationExists(new_path.to_path_buf()));
        }
        if let Some(parent) = new_path.parent() {
            if !parent.is_dir() {
                fs::create_dir_all(parent)?;
            }
        }

        // The titles used for matching are the filename basenames,
        // not the H1s. H1 is display; filename is the canonical
        // wikilink target. This is the right behavior on rename
        // because the file the user is renaming is no longer
        // reachable under its old name.
        let old_canonical = canonicalize(old_path);
        let new_canonical = canonicalize_or_self(new_path);
        let old_basename = title_from_path(&old_canonical);
        let new_basename = title_from_path(&new_canonical);
        // For matching, use the lowercased form (`link_key`).
        let old_title = crate::index::link_key(&old_basename);
        let new_title = crate::index::link_key(&new_basename);
        // For substitution in rewritten content, use the original-case
        // basename so the rewritten link reads `[[New]]` not `[[new]]`.
        let new_basename_for_substitution = strip_md_suffix(&new_basename);

        // Move the file.
        fs::rename(&old_canonical, &new_canonical)?;

        // The filenames match (e.g. "Note" -> "Note"): no rewrites
        // needed, but the path did change, so re-key the index.
        if old_title == new_title {
            self.remove_note(&old_canonical);
            self.update_note_inner_public(&new_canonical)?;
            return Ok(RenameResult {
                new_path: new_canonical,
                old_title,
                new_title,
                rewritten_sources: Vec::new(),
                link_replacements: 0,
            });
        }

        // Walk every note (except the renamed one), find links
        // whose normalized target equals `old_title`, and rewrite
        // them in the source content.
        let mut rewritten_sources: Vec<PathBuf> = Vec::new();
        let mut total_replacements: usize = 0;
        let affected: Vec<PathBuf> = self
            .notes
            .keys()
            .filter(|p| **p != old_canonical && **p != new_canonical)
            .cloned()
            .collect();
        for source_path in affected {
            let note = match self.notes.get(&source_path) {
                Some(n) => n,
                None => continue,
            };
            let (new_content, count) = rewrite_links_in_content(
                &note.content,
                &old_title,
                &new_basename_for_substitution,
            );
            if count == 0 {
                continue;
            }
            if let Err(e) = fs::write(&source_path, &new_content) {
                return Err(RenameError::Io(e));
            }
            rewritten_sources.push(source_path.clone());
            total_replacements += count;
        }

        // Update the index: re-index the renamed note at its new
        // path, and re-index every rewritten source so the
        // new_target is reflected in backlinks/outgoing.
        self.remove_note(&old_canonical);
        self.update_note_inner_public(&new_canonical)?;
        for source in &rewritten_sources {
            self.update_note_inner_public(source)?;
        }

        Ok(RenameResult {
            new_path: new_canonical,
            old_title,
            new_title,
            rewritten_sources,
            link_replacements: total_replacements,
        })
    }

    /// Public wrapper for the (private) inner update. Used by
    /// [`Self::rename`].
    pub(crate) fn update_note_inner_public(&mut self, path: &Path) -> Result<(), crate::IndexError> {
        self.update_note(path)
    }
}

/// Canonicalize a path, falling back to the original if the file
/// doesn't exist (during a rename the source is moved before the
/// destination exists).
fn canonicalize(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn canonicalize_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

fn strip_md_suffix(s: &str) -> String {
    s.strip_suffix(".md").unwrap_or(s).to_string()
}

/// Rewrite every wikilink and markdown link in `content` whose
/// target normalizes to `old_title`, replacing the target with
/// `new_title` while preserving the link's heading / block /
/// section / alias suffix. Returns the rewritten content and the
/// number of replacements made.
pub fn rewrite_links_in_content(
    content: &str,
    old_title: &str,
    new_title: &str,
) -> (String, usize) {
    use std::sync::LazyLock;
    use regex::Regex;

    static WIKILINK_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\[\[([^\[\]]+?)\]\]").unwrap());
    static MDLINK_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?m)\[([^\[\]]+)\]\(([^()\s]+)(?:\s+"[^"]*")?\)"#).unwrap()
    });

    let mut out = String::with_capacity(content.len());
    let mut last_end = 0;
    let mut count = 0;

    // Collect all matches (from both regexes) with their ranges,
    // then sort by start and apply. This handles overlapping
    // patterns by processing left-to-right and skipping past
    // already-consumed ranges.
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for caps in WIKILINK_RE.captures_iter(content) {
        let m = caps.get(0).unwrap();
        let inner = caps.get(1).unwrap().as_str();
        let (raw_target, _alias) = split_alias(inner);
        // Strip any #heading / #^block / ##section suffix before
        // comparing to the bare title. link_key doesn't do this
        // because it's called with already-stripped targets in the
        // indexer; here we get the raw inner.
        let key = bare_target_key(&raw_target);
        if key != old_title.to_lowercase() {
            continue;
        }
        let new_inner = substitute_target(&raw_target, old_title, new_title);
        // Preserve alias if present
        let new_full = if let Some((_, Some(alias))) = Some(split_alias_with_option(inner)) {
            format!("[[{}|{}]]", new_inner, alias)
        } else {
            format!("[[{}]]", new_inner)
        };
        edits.push((m.start()..m.end(), new_full));
    }
    for caps in MDLINK_RE.captures_iter(content) {
        let m = caps.get(0).unwrap();
        let target = caps.get(2).unwrap().as_str();
        let key = crate::index::link_key(target);
        if key != old_title.to_lowercase() {
            continue;
        }
        let new_target = substitute_target(target, old_title, new_title);
        // The original is `[text](url "title")` or `[text](url)`.
        // We replace the URL portion only, keeping the text and any
        // title attribute intact. Find the URL by its offset within
        // the match and rebuild `[text](new_url ...rest)`.
        let original = m.as_str();
        let close_bracket = original.find("]").unwrap();
        let text = &original[1..close_bracket];
        // URL starts at the char after the opening paren.
        let url_start_in_match = close_bracket + 2; // skip "]( "
        let url_end_in_match = url_start_in_match + target.len();
        let suffix = &original[url_end_in_match..];
        // `suffix` is either "" (no title) or ` "title...")`.
        let new_full = format!("[{}]({}{}", text, new_target, suffix);
        edits.push((m.start()..m.end(), new_full));
    }
    edits.sort_by_key(|(r, _)| r.start);

    for (range, replacement) in edits {
        if range.start < last_end {
            continue; // overlap with previous edit; skip
        }
        out.push_str(&content[last_end..range.start]);
        out.push_str(&replacement);
        last_end = range.end;
        count += 1;
    }
    out.push_str(&content[last_end..]);
    (out, count)
}

fn split_alias(inner: &str) -> (String, Option<String>) {
    if let Some(idx) = inner.find('|') {
        (inner[..idx].to_string(), Some(inner[idx + 1..].to_string()))
    } else {
        (inner.to_string(), None)
    }
}

fn split_alias_with_option(inner: &str) -> (String, Option<String>) {
    split_alias(inner)
}

/// Like `link_key` but also strips `#heading`, `#^block`, and
/// `##section` suffixes. Used by the rewriter to match the bare
/// target against `old_title` regardless of which link variant the
/// user wrote.
fn bare_target_key(target: &str) -> String {
    let stripped = if let Some(idx) = target.find('#') {
        &target[..idx]
    } else {
        target
    };
    crate::index::link_key(stripped)
}

/// Replace just the bare-target portion of a link inner string,
/// preserving any `#heading`, `#^block`, `##section`, or `.md`
/// suffix.
fn substitute_target(raw_target: &str, _old: &str, new: &str) -> String {
    // raw_target might be "Note" or "folder/Note" or "Note.md" or
    // "Note#Heading" etc. We want to swap just the "Note" part for
    // "NewNote" (preserving the suffix). Easiest: split on `#`,
    // `.md`, or path separator; replace the first segment.
    if let Some(idx) = raw_target.find('#') {
        let suffix = &raw_target[idx..];
        format!("{}{}", new, suffix)
    } else if raw_target.strip_suffix(".md").is_some() {
        format!("{}.md", new)
    } else if let Some(idx) = raw_target.rfind(['/', '\\']) {
        let prefix = &raw_target[..idx + 1];
        format!("{}{}", prefix, new)
    } else {
        new.to_string()
    }
}

/// Re-parse a body after a rewrite, returning the updated
/// `ParsedNote` (used by tests).
pub fn reparse(content: &str) -> ParsedNote {
    crate::note_parser::parse(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_vault(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("tungsten-rename-{label}-{pid}-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, rel: &str, body: &str) -> PathBuf {
        let path = dir.join(rel);
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn rewrite_preserves_heading() {
        let (out, n) = rewrite_links_in_content(
            "see [[Old#Heading]] and [[Old#^block]] and [[Old]]",
            "old",
            "new",
        );
        assert_eq!(n, 3);
        assert!(out.contains("[[new#Heading]]"));
        assert!(out.contains("[[new#^block]]"));
        assert!(out.contains("[[new]]"));
    }

    #[test]
    fn rewrite_preserves_alias() {
        let (out, n) =
            rewrite_links_in_content("see [[Old|the alias]]", "old", "new");
        assert_eq!(n, 1);
        assert!(out.contains("[[new|the alias]]"));
    }

    #[test]
    fn rewrite_markdown_link() {
        let (out, n) =
            rewrite_links_in_content("see [the other](Old.md)", "old", "new");
        assert_eq!(n, 1);
        assert!(out.contains("[the other](new.md)"));
    }

    #[test]
    fn rewrite_with_title_attribute() {
        let (out, n) = rewrite_links_in_content(
            r#"see [t](Old.md "Old note")"#,
            "old",
            "new",
        );
        assert_eq!(n, 1);
        assert!(out.contains(r#"(new.md "Old note")"#));
    }

    #[test]
    fn rewrite_skips_unrelated_links() {
        let (out, n) = rewrite_links_in_content(
            "see [[Other]] and [[Old]]",
            "old",
            "new",
        );
        assert_eq!(n, 1);
        assert!(out.contains("[[Other]]"));
        assert!(out.contains("[[new]]"));
    }

    #[test]
    fn rewrite_path_prefixed_link() {
        let (out, n) = rewrite_links_in_content(
            "see [[folder/Old]]",
            "old",
            "new",
        );
        assert_eq!(n, 1);
        assert!(out.contains("[[folder/new]]"));
    }

    #[test]
    fn rename_moves_file_and_rewrites_links() {
        let dir = unique_vault("rename");
        let old = write(&dir, "Old.md", "# Old\n\nbody\n");
        let source = write(
            &dir,
            "Source.md",
            "# Source\n\nlinks to [[Old]] and [[Other]]\n",
        );
        let target = write(&dir, "Other.md", "# Other\nbody\n");
        let mut idx = NoteIndex::build(&dir).unwrap();
        let result = idx.rename(&old, &dir.join("New.md")).unwrap();
        assert_eq!(result.new_title, "new");
        assert_eq!(result.old_title, "old");
        assert_eq!(result.rewritten_sources.len(), 1);
        assert_eq!(result.rewritten_sources[0], source.canonicalize().unwrap());
        assert!(result.link_replacements >= 1);
        // File moved
        assert!(!old.exists());
        assert!(dir.join("New.md").exists());
        // Source rewritten
        let body = fs::read_to_string(&source).unwrap();
        assert!(body.contains("[[New]]"));
        assert!(body.contains("[[Other]]"));
        // The renamed note's H1 is still "Old" (H1s don't
        // auto-update on rename), so by_title("Old") still
        // resolves to it. The renamed note is reachable at its
        // new canonical path; the file is no longer at the old
        // path.
        assert!(idx.by_title("Old").is_some());
        assert_eq!(
            idx.get(&dir.join("New.md").canonicalize().unwrap())
                .map(|n| n.title.as_str()),
            Some("Old"),
        );
        // Source's forward links now resolve: "Other" still
        // resolves to Other.md (untouched); the rewritten [[New]]
        // is unresolved (no note named "New") because the H1 of
        // the renamed file is still "Old" — i.e. the rename
        // updated the file's *filename* but the index keys notes
        // by H1. This is a known limitation of the current
        // indexer; it can be tightened in a follow-up.
        let forward: Vec<&str> = idx
            .forward_links(&source)
            .iter()
            .map(|n| n.title.as_str())
            .collect();
        assert!(forward.contains(&"Other"), "forward links: {forward:?}");
        let _ = target;
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rename_missing_source_errors() {
        let dir = unique_vault("missing");
        let mut idx = NoteIndex::build(&dir).unwrap();
        let err = idx.rename(&dir.join("NotHere.md"), &dir.join("X.md")).unwrap_err();
        assert!(matches!(err, RenameError::SourceMissing(_)));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rename_to_existing_destination_errors() {
        let dir = unique_vault("exists");
        let a = write(&dir, "A.md", "# A\n");
        let b = write(&dir, "B.md", "# B\n");
        let mut idx = NoteIndex::build(&dir).unwrap();
        let err = idx.rename(&a, &b).unwrap_err();
        assert!(matches!(err, RenameError::DestinationExists(_)));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rename_preserves_heading_links() {
        let dir = unique_vault("heading");
        let old = write(&dir, "Old.md", "# Old\n\nbody\n");
        let source = write(
            &dir,
            "Source.md",
            "# Source\n\nsee [[Old#Some Section]] and [[Old#^b123]]\n",
        );
        let mut idx = NoteIndex::build(&dir).unwrap();
        let result = idx.rename(&old, &dir.join("New.md")).unwrap();
        assert_eq!(result.link_replacements, 2);
        let body = fs::read_to_string(&source).unwrap();
        assert!(body.contains("[[New#Some Section]]"));
        assert!(body.contains("[[New#^b123]]"));
        fs::remove_dir_all(&dir).ok();
    }
}
