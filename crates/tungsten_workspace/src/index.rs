//! In-memory note index for a vault.
//!
//! Built from the vault root by walking the directory tree, reading
//! every `.md` file, and parsing it into a [`Note`]. The index keeps
//! four denormalized lookups on top of the canonical `BTreeMap<Path,
//! Note>`:
//!
//! 1. `by_title_lower` — case-insensitive note title → path. Used
//!    to resolve `[[Wikilink]]` targets and unlinked-mention hits.
//! 2. `by_tag` — tag → set of paths. Used by the Tags panel.
//! 3. `backlinks` — lowercased target title → set of source paths.
//!    Used by the Backlinks panel.
//! 4. `outgoing` — path → set of lowercased targets referenced by
//!    that note. Used for orphan detection and forward-link lookup.
//!
//! The index is fully synchronous and lives in process memory. SQLite
//! persistence (the M1.2 sidecar DB) is a later commit; the
//! in-memory representation is the source of truth and is rebuilt
//! from disk on every `build()` / `rebuild()`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::note::{Note, UnlinkedMention};

/// Subdirectories of the vault root that the indexer skips. Obsidian
/// stores its config in `.obsidian/` and trashed notes in `.trash/`;
/// both should not show up in the graph, the tags panel, etc.
pub const IGNORED_DIRS: &[&str] = &[".obsidian", ".trash", ".git", "node_modules"];

/// Errors that can occur while building or updating the index.
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("failed to read note {path}: {source}")]
    NoteRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Summary statistics about the index. Cheap to compute.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct IndexStats {
    pub note_count: usize,
    pub link_count: usize,
    pub tag_count: usize,
    pub orphan_count: usize,
}

/// The note index. See module docs.
#[derive(Debug, Default, Clone)]
pub struct NoteIndex {
    pub(crate) notes: BTreeMap<PathBuf, Note>,
    /// Case-insensitive title → path. Keys are the note's H1
    /// (lower-cased) AND the file's basename (lower-cased, with
    /// `.md` stripped). Either lookup returns the same path.
    pub(crate) by_title_lower: HashMap<String, PathBuf>,
    pub(crate) by_tag: BTreeMap<String, BTreeSet<PathBuf>>,
    /// Backlinks: lowercased *target title* (the part of a wikilink
    /// before any `#` or `|` separator) → set of source paths.
    pub(crate) backlinks: HashMap<String, BTreeSet<PathBuf>>,
    /// Outgoing: source path → set of lowercased target titles
    /// referenced from that note. Used for orphan detection and
    /// forward-link resolution.
    pub(crate) outgoing: BTreeMap<PathBuf, BTreeSet<String>>,
}

impl NoteIndex {
    /// Build a fresh index by walking `vault_root`. Returns an empty
    /// index if the root does not exist or contains no `.md` files.
    pub fn build(vault_root: &Path) -> Result<Self, IndexError> {
        let mut index = Self::default();
        index.rebuild(vault_root)?;
        Ok(index)
    }

    /// Re-scan the entire vault and replace the index contents.
    pub fn rebuild(&mut self, vault_root: &Path) -> Result<(), IndexError> {
        self.notes.clear();
        self.by_title_lower.clear();
        self.by_tag.clear();
        self.backlinks.clear();
        self.outgoing.clear();
        for entry in WalkDir::new(vault_root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_ignored(e.path(), vault_root))
        {
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            // canonicalize so paths are comparable
            let path = match path.canonicalize() {
                Ok(p) => p,
                Err(_) => path.to_path_buf(),
            };
            self.update_note_inner(&path)?;
        }
        Ok(())
    }

    /// Re-read and re-index a single note. Useful when a file
    /// changes on disk.
    pub fn update_note(&mut self, path: &Path) -> Result<(), IndexError> {
        let path = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf());
        self.update_note_inner(&path)
    }

    fn update_note_inner(&mut self, path: &Path) -> Result<(), IndexError> {
        // Remove any existing entry first so stale data is dropped.
        self.remove_note_inner(path);

        let note = match Note::read(path) {
            Ok(n) => n,
            Err(source) => {
                return Err(IndexError::NoteRead {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };

        let title_lower = note.title.to_lowercase();
        self.by_title_lower.insert(title_lower.clone(), path.to_path_buf());
        // Also index the file's basename (without .md). This
        // means `by_title("new")` finds a file `New.md` even when
        // its H1 is "Old" — and after a rename, `by_title("New")`
        // finds the newly-named file regardless of its H1.
        if let Some(basename) = path.file_stem().and_then(|s| s.to_str()) {
            let basename_lower = basename.to_lowercase();
            if basename_lower != title_lower {
                self.by_title_lower
                    .entry(basename_lower)
                    .or_insert(path.to_path_buf());
            }
        }
        for tag in &note.tags {
            self.by_tag
                .entry(tag.clone())
                .or_default()
                .insert(path.to_path_buf());
        }

        let mut targets = BTreeSet::new();
        for link in &note.links {
            // The parser already stripped the `#` and `|`; `target`
            // is the bare note name (or path-with-.md). For backlink
            // indexing we key by the basename without extension so
            // `[[folder/Note]]` and `[[Note]]` and `[[Note.md]]` all
            // resolve to the same key.
            let key = link_key(&link.target);
            if key.is_empty() {
                continue;
            }
            self.backlinks
                .entry(key.clone())
                .or_default()
                .insert(path.to_path_buf());
            targets.insert(key);
        }
        self.outgoing.insert(path.to_path_buf(), targets);
        self.notes.insert(path.to_path_buf(), note);
        Ok(())
    }

    fn remove_note_inner(&mut self, path: &Path) {
        if let Some(removed) = self.notes.remove(path) {
            let title_lower = removed.title.to_lowercase();
            if self.by_title_lower.get(&title_lower) == Some(&path.to_path_buf()) {
                self.by_title_lower.remove(&title_lower);
            }
            for tag in &removed.tags {
                if let Some(set) = self.by_tag.get_mut(tag) {
                    set.remove(path);
                    if set.is_empty() {
                        self.by_tag.remove(tag);
                    }
                }
            }
            if let Some(targets) = self.outgoing.remove(path) {
                for target in targets {
                    if let Some(set) = self.backlinks.get_mut(&target) {
                        set.remove(path);
                        if set.is_empty() {
                            self.backlinks.remove(&target);
                        }
                    }
                }
            }
        }
    }

    /// Remove a note from the index. Returns `true` if a note was
    /// actually removed.
    pub fn remove_note(&mut self, path: &Path) -> bool {
        let path = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf());
        let existed = self.notes.contains_key(&path);
        self.remove_note_inner(&path);
        existed
    }

    /// All notes in the index, in deterministic (path) order.
    pub fn notes(&self) -> impl Iterator<Item = &Note> {
        self.notes.values()
    }

    /// Look up a note by absolute path.
    pub fn get(&self, path: &Path) -> Option<&Note> {
        let path = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf());
        self.notes.get(&path)
    }

    /// Look up a note by title (case-insensitive). The title is the
    /// note's first H1 or filename, as returned by [`Note::title`].
    pub fn by_title(&self, title: &str) -> Option<&Note> {
        let key = title.to_lowercase();
        self.by_title_lower
            .get(&key)
            .and_then(|p| self.notes.get(p))
    }

    /// All notes that have the given tag (case-insensitive).
    pub fn with_tag(&self, tag: &str) -> impl Iterator<Item = &Note> {
        let key = tag.to_lowercase();
        self.by_tag
            .get(&key)
            .into_iter()
            .flat_map(|set| set.iter())
            .filter_map(|p| self.notes.get(p))
    }

    /// All distinct tags in the index, in sorted order.
    pub fn tags(&self) -> impl Iterator<Item = &str> {
        self.by_tag.keys().map(|s| s.as_str())
    }

    /// All notes that link to a given target (case-insensitive). The
    /// target is matched against the bare note name (path and
    /// `.md` extension are normalized away).
    pub fn backlinks(&self, target: &str) -> impl Iterator<Item = &Note> {
        let key = link_key(target);
        self.backlinks
            .get(&key)
            .into_iter()
            .flat_map(|set| set.iter())
            .filter_map(|p| self.notes.get(p))
    }

    /// All notes that the source note links to, resolved by title.
    /// Links that don't resolve to a known note are dropped.
    pub fn forward_links<'a>(&'a self, source: &Path) -> Vec<&'a Note> {
        let source = source
            .canonicalize()
            .unwrap_or_else(|_| source.to_path_buf());
        let Some(targets) = self.outgoing.get(&source) else {
            return Vec::new();
        };
        targets
            .iter()
            .filter_map(|t| self.by_title_lower.get(t).and_then(|p| self.notes.get(p)))
            .collect()
    }

    /// All notes that have no incoming AND no outgoing resolved
    /// links. (Unresolved links are ignored — they may be created
    /// later via "Create new note" on a broken wikilink.)
    pub fn orphans(&self) -> Vec<&Note> {
        self.notes
            .values()
            .filter(|n| {
                let no_outgoing = self
                    .outgoing
                    .get(&n.path)
                    .map(|s| s.is_empty())
                    .unwrap_or(true);
                let no_incoming = !self
                    .backlinks
                    .values()
                    .any(|set| set.contains(&n.path));
                no_outgoing && no_incoming
            })
            .collect()
    }

    /// All plain-text occurrences of `target_title` in any note that
    /// is not already a link to that note. Excludes code spans and
    /// code blocks.
    pub fn unlinked_mentions(&self, target_title: &str) -> Vec<UnlinkedMention> {
        let needle = target_title.to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for note in self.notes.values() {
            // Don't match against a note's own title (it always
            // "mentions" itself).
            if note.title.to_lowercase() == needle {
                continue;
            }
            find_mentions_in_note(note, target_title, &mut out);
        }
        out
    }

    /// Summary statistics. Computed on demand.
    pub fn stats(&self) -> IndexStats {
        let link_count: usize = self.outgoing.values().map(|s| s.len()).sum();
        let tag_count = self.by_tag.len();
        let orphan_count = self.orphans().len();
        IndexStats {
            note_count: self.notes.len(),
            link_count,
            tag_count,
            orphan_count,
        }
    }

    /// Total notes in the index.
    pub fn len(&self) -> usize {
        self.notes.len()
    }

    /// Whether the index contains zero notes.
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }
}

/// Normalize a link target for backlink lookups. Strips a `.md`
/// extension and a leading `./` so that `[[Note]]`, `[[Note.md]]`,
/// `[[./Note]]`, and `[[folder/Note]]` all resolve to the same key.
pub(crate) fn link_key(target: &str) -> String {
    let mut t = target.trim().to_string();
    if let Some(stripped) = t.strip_suffix(".md") {
        t = stripped.to_string();
    }
    if let Some(stripped) = t.strip_prefix("./") {
        t = stripped.to_string();
    }
    // Use just the basename (last path component) so that links
    // like `[[journal/2026-07-12]]` still resolve to a note called
    // "2026-07-12" in the same folder. (We use the basename; if
    // multiple notes have the same basename in different folders,
    // we accept the ambiguity — same as Obsidian does.)
    if let Some(idx) = t.rfind(['/', '\\']) {
        t = t[idx + 1..].to_string();
    }
    t.to_lowercase()
}

/// Whether a path lives under an ignored directory (relative to the
/// vault root). Used by [`WalkDir::filter_entry`].
fn is_ignored(path: &Path, vault_root: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(vault_root) else {
        return true;
    };
    for component in rel.components() {
        if let std::path::Component::Normal(s) = component {
            if let Some(s) = s.to_str() {
                if IGNORED_DIRS.contains(&s) {
                    return true;
                }
            }
        }
    }
    false
}

/// Scan one note's content for plain-text occurrences of `target`.
/// Excludes code spans, code blocks, and any byte range already
/// covered by a link.
fn find_mentions_in_note(
    note: &Note,
    target: &str,
    out: &mut Vec<UnlinkedMention>,
) {
    let lower_content = note.content.to_lowercase();
    let needle = target.to_lowercase();
    if needle.is_empty() {
        return;
    }

    // Compute the link-ranges (in bytes) so we can skip them.
    let link_ranges: Vec<std::ops::Range<usize>> =
        note.links.iter().map(|l| l.byte_range.clone()).collect();

    // Also exclude code ranges — recompute by walking the parser.
    // (This is duplicative with the parser's collect_code_ranges
    // but kept local because that helper is private.)
    let code_ranges = code_ranges(&note.content);

    let mut start = 0usize;
    while let Some(pos) = lower_content[start..].find(&needle) {
        let abs = start + pos;
        start = abs + needle.len();
        // Skip if inside a link
        if link_ranges.iter().any(|r| abs >= r.start && abs < r.end) {
            continue;
        }
        // Skip if inside a code region
        if code_ranges.iter().any(|(s, e)| abs >= *s && abs < *e) {
            continue;
        }
        // Word boundary on the left: a mention is a real mention
        // only if the preceding character is non-alphanumeric. (We
        // accept the next character being anything — it might be a
        // punctuation mark that's part of the surrounding sentence.)
        if abs > 0 {
            let prev = note.content.as_bytes()[abs - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'-' {
                continue;
            }
        }
        out.push(UnlinkedMention {
            source: note.path.clone(),
            byte_range: abs..abs + needle.len(),
        });
    }
}

fn code_ranges(body: &str) -> Vec<(usize, usize)> {
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
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_vault(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("tungsten-idx-{label}-{pid}-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, rel: &str, body: &str) -> PathBuf {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn link_key_strips_md_and_normalizes() {
        assert_eq!(link_key("Note"), "note");
        assert_eq!(link_key("Note.md"), "note");
        assert_eq!(link_key("./Note"), "note");
        assert_eq!(link_key("folder/Note.md"), "note");
        assert_eq!(link_key("Folder/Note"), "note");
    }

    #[test]
    fn empty_vault_yields_empty_index() {
        let dir = unique_vault("empty");
        let idx = NoteIndex::build(&dir).unwrap();
        assert_eq!(idx.len(), 0);
        assert!(idx.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn build_indexes_simple_vault() {
        let dir = unique_vault("simple");
        write(
            &dir,
            "Alpha.md",
            "# Alpha\n\nlinks to [[Beta]] and [[Gamma]].\n",
        );
        write(
            &dir,
            "Beta.md",
            "# Beta\n\nalpha: see [[Alpha]].\n",
        );
        write(&dir, "Gamma.md", "# Gamma\n\nisolated note.\n");
        let idx = NoteIndex::build(&dir).unwrap();
        assert_eq!(idx.len(), 3);
        let alpha = idx.by_title("Alpha").unwrap();
        assert_eq!(alpha.path.file_name().unwrap(), "Alpha.md");
        // backlinks to Alpha = [Beta]
        let bl: Vec<&str> = idx.backlinks("Alpha").map(|n| n.title.as_str()).collect();
        assert_eq!(bl, vec!["Beta"]);
        // backlinks to Beta = [Alpha]
        let bl: Vec<&str> = idx.backlinks("Beta").map(|n| n.title.as_str()).collect();
        assert_eq!(bl, vec!["Alpha"]);
        // Gamma is orphan
        let orphans: Vec<&str> = idx.orphans().iter().map(|n| n.title.as_str()).collect();
        assert!(orphans.contains(&"Gamma"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn build_skips_ignored_dirs() {
        let dir = unique_vault("ignored");
        write(&dir, "Real.md", "# Real\nbody\n");
        write(&dir, ".obsidian/notes.md", "should be ignored");
        write(&dir, ".trash/old.md", "should also be ignored");
        let idx = NoteIndex::build(&dir).unwrap();
        assert_eq!(idx.len(), 1);
        assert!(idx.by_title("Real").is_some());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tags_indexed_from_frontmatter_and_inline() {
        let dir = unique_vault("tags");
        write(
            &dir,
            "A.md",
            "---\ntags: [rust, perf]\n---\n# A\n\nbody with #rust and #cli\n",
        );
        let idx = NoteIndex::build(&dir).unwrap();
        let mut got: Vec<&str> = idx.with_tag("rust").map(|n| n.title.as_str()).collect();
        got.sort();
        assert_eq!(got, vec!["A"]);
        let tags: Vec<&str> = idx.tags().collect();
        assert!(tags.contains(&"rust"));
        assert!(tags.contains(&"perf"));
        assert!(tags.contains(&"cli"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn forward_links_resolve_via_title() {
        let dir = unique_vault("forward");
        write(
            &dir,
            "Source.md",
            "# Source\n\nsee [[Target]] and [[nonexistent]]\n",
        );
        write(&dir, "Target.md", "# Target\nbody\n");
        let idx = NoteIndex::build(&dir).unwrap();
        let source_path = dir.join("Source.md");
        let forward: Vec<&str> = idx
            .forward_links(&source_path)
            .iter()
            .map(|n| n.title.as_str())
            .collect();
        assert_eq!(forward, vec!["Target"], "unresolved links are dropped");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unlinked_mentions_skip_existing_links_and_code() {
        let dir = unique_vault("mentions");
        write(&dir, "Target.md", "# Target\nbody\n");
        write(
            &dir,
            "Source.md",
            "# Source\n\nI mention Target in text. Also a [[Target|link]] \
             and a `Target in code` and a #not-a-mention.\n",
        );
        let idx = NoteIndex::build(&dir).unwrap();
        let mentions = idx.unlinked_mentions("Target");
        // Only the plain-text "Target" hit should be returned, not
        // the wikilink one, not the code-span one.
        assert_eq!(mentions.len(), 1, "got: {mentions:?}");
        assert!(mentions[0].source.ends_with("Source.md"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_note_picks_up_edits() {
        let dir = unique_vault("update");
        let path = write(&dir, "Note.md", "# Note\n\nfirst version\n");
        let mut idx = NoteIndex::build(&dir).unwrap();
        assert_eq!(idx.len(), 1);
        fs::write(&path, "# Note\n\nsecond version with #newtag\n").unwrap();
        idx.update_note(&path).unwrap();
        assert_eq!(idx.len(), 1);
        let note = idx.get(&path).unwrap();
        assert!(note.content.contains("second version"));
        let new_tag: Vec<&str> = idx.with_tag("newtag").map(|n| n.title.as_str()).collect();
        assert_eq!(new_tag, vec!["Note"]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn remove_note_drops_from_indexes() {
        let dir = unique_vault("remove");
        let path = write(&dir, "Lonely.md", "# Lonely\n\nlinks to [[Target]]\n");
        write(&dir, "Target.md", "# Target\n");
        let mut idx = NoteIndex::build(&dir).unwrap();
        assert_eq!(idx.len(), 2);
        // Lonely has backlinks to Target from... wait, Lonely links to
        // Target, not the other way around. So removing Lonely should
        // not affect Target's backlinks.
        idx.remove_note(&path);
        assert_eq!(idx.len(), 1);
        assert!(idx.get(&path).is_none());
        let bl: Vec<&str> = idx.backlinks("Target").map(|n| n.title.as_str()).collect();
        assert!(bl.is_empty(), "no one links to Target after removal");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn by_title_finds_by_filename_when_h1_differs() {
        let dir = unique_vault("filename-lookup");
        // Note whose H1 is "Daily" but the file is "Journal.md".
        write(
            &dir,
            "Journal.md",
            "# Daily\n\nbody\n",
        );
        let idx = NoteIndex::build(&dir).unwrap();
        // by_title should find it by either H1 or filename.
        assert!(idx.by_title("Daily").is_some());
        assert!(
            idx.by_title("journal").is_some(),
            "by_title should resolve by filename basename (case-insensitive)"
        );
        assert!(idx.by_title("Journal").is_some());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn by_title_finds_by_filename_after_rename() {
        let dir = unique_vault("rename-lookup");
        // Note whose H1 matches its filename; rename the file.
        let old = write(&dir, "OldName.md", "# OldName\nbody\n");
        let mut idx = NoteIndex::build(&dir).unwrap();
        assert!(idx.by_title("OldName").is_some());
        assert!(idx.by_title("oldname").is_some());
        idx.rename(&old, &dir.join("NewName.md")).unwrap();
        // After the rename, the file's basename is "NewName" but
        // its H1 is still "OldName" (H1s don't auto-update on
        // rename in Obsidian). Both lookups should still work.
        assert!(
            idx.by_title("NewName").is_some(),
            "by_title should find the renamed file by its new filename"
        );
        assert!(
            idx.by_title("newname").is_some(),
            "case-insensitive filename lookup"
        );
        assert!(
            idx.by_title("OldName").is_some(),
            "by_title should still find the renamed file by its old H1"
        );
        // Backlinks of the new filename point at the renamed file.
        // (We didn't write any backlinks, so this is empty.)
        let _ = idx.backlinks("NewName").count();
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn build_real_obsidian_vault() {
        // The CI machine has a sample vault. We tolerate absence
        // (the test passes with 0 notes) so this stays portable.
        let candidates = [
            std::path::PathBuf::from("/home/fuego/Documents/Notes"),
            std::path::PathBuf::from("./demo-vault"),
        ];
        let vault = candidates.into_iter().find(|p| p.join(".obsidian").is_dir());
        let Some(vault) = vault else {
            return;
        };
        let idx = NoteIndex::build(&vault).unwrap();
        let stats = idx.stats();
        assert!(stats.note_count > 0, "real vault should have notes");
        // Sanity: every note's links should parse without panic.
        for n in idx.notes() {
            for l in &n.links {
                let _ = l.target.len();
            }
        }
    }
}
