//! Resource providers: attachment indexing.
//!
//! A vault contains both Markdown notes and "attachments" — any
//! non-`.md` file the user has placed in the vault. The most
//! common kinds are images (referenced from notes via
//! `![[image.png]]`), PDFs (previewed in-page), and audio
//! recordings (the daily-journal audio widget). This module
//! indexes those files and provides lookup APIs.
//!
//! **Scope:** the index is in-memory and rebuilt on demand; a
//! SQLite-backed sidecar is a future commit (the schema is
//! straightforward — one `attachments` table mirroring the rows
//! of [`Attachment`]).
//!
//! **Out of scope:** thumbnails, full-text extraction from
//! PDFs, EXIF parsing for images, waveform analysis for audio.
//! Those are renderer/editor concerns, not indexer concerns.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use walkdir::WalkDir;

use crate::index::IGNORED_DIRS;

/// A non-`.md` file in a vault, indexed for lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// Absolute path to the file on disk.
    pub path: PathBuf,
    /// Basename including extension, e.g. `"diagram.png"`.
    pub name: String,
    /// Lowercase extension without the dot, e.g. `"png"`.
    pub extension: String,
    /// Best-effort MIME type, e.g. `"image/png"`. Unknown
    /// extensions yield `"application/octet-stream"`.
    pub mime_type: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Last-modified time, if the filesystem reported one.
    pub mtime: Option<SystemTime>,
}

impl Attachment {
    /// High-level kind of the attachment. Used by the editor to
    /// pick a previewer / handler.
    pub fn kind(&self) -> AttachmentKind {
        AttachmentKind::from_extension(&self.extension)
    }
}

/// The high-level category of an attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    Image,
    Video,
    Audio,
    Pdf,
    Document,
    Other,
}

impl AttachmentKind {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "avif" => {
                AttachmentKind::Image
            }
            "mp4" | "webm" | "mov" | "mkv" | "avi" | "m4v" | "ogv" => AttachmentKind::Video,
            "mp3" | "wav" | "ogg" | "m4a" | "flac" | "opus" | "aac" | "oga" => AttachmentKind::Audio,
            "pdf" => AttachmentKind::Pdf,
            "doc" | "docx" | "odt" | "rtf" | "txt" | "md" => AttachmentKind::Document,
            _ => AttachmentKind::Other,
        }
    }
}

/// In-memory index of attachments in a vault.
#[derive(Debug, Default, Clone)]
pub struct AttachmentIndex {
    attachments: HashMap<PathBuf, Attachment>,
    by_name_lower: HashMap<String, PathBuf>,
}

impl AttachmentIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the index by walking `vault_root`. Skips ignored
    /// directories (same set the [`crate::NoteIndex`] uses).
    pub fn build(vault_root: &Path) -> Self {
        let mut index = Self::default();
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
            // Skip the .md files — those are notes, not
            // attachments — and skip our own sidecar index DB.
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext == "md" {
                continue;
            }
            // `.tungsten/index.db` lives inside the vault and is
            // not an attachment.
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n == "index.db")
                .unwrap_or(false)
            {
                continue;
            }
            let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            let _ = index.update_path(&canonical);
        }
        index
    }

    /// Re-read a single attachment. Returns `true` if the file
    /// exists and was added; `false` otherwise.
    pub fn update(&mut self, path: &Path) -> bool {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !canonical.is_file() {
            return false;
        }
        self.update_path(&canonical).is_some()
    }

    fn update_path(&mut self, path: &Path) -> Option<()> {
        // Drop any existing entry under this path so the
        // by_name index doesn't get a stale mapping.
        if let Some(prev) = self.attachments.remove(path) {
            self.by_name_lower.remove(&prev.name.to_lowercase());
        }
        let metadata = std::fs::metadata(path).ok()?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let extension = path
            .extension()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        let mime_type = mime_from_extension(&extension);
        let size_bytes = metadata.len();
        let mtime = metadata.modified().ok();
        let attachment = Attachment {
            path: path.to_path_buf(),
            name: name.clone(),
            extension: extension.clone(),
            mime_type,
            size_bytes,
            mtime,
        };
        self.by_name_lower
            .insert(name.to_lowercase(), path.to_path_buf());
        self.attachments.insert(path.to_path_buf(), attachment);
        Some(())
    }

    /// Remove an attachment by path. Returns `true` if it was
    /// present.
    pub fn remove(&mut self, path: &Path) -> bool {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(removed) = self.attachments.remove(&canonical) {
            self.by_name_lower.remove(&removed.name.to_lowercase());
            true
        } else {
            false
        }
    }

    /// Look up an attachment by absolute path.
    pub fn get(&self, path: &Path) -> Option<&Attachment> {
        let canonical = path.canonicalize().ok()?;
        self.attachments.get(&canonical)
    }

    /// Look up an attachment by filename (basename, case-insensitive).
    /// Returns the first match if multiple attachments share a
    /// name (which can happen in different folders); caller can
    /// disambiguate via [`Self::all_by_name`].
    pub fn by_name(&self, name: &str) -> Option<&Attachment> {
        let key = name.to_lowercase();
        self.by_name_lower
            .get(&key)
            .and_then(|p| self.attachments.get(p))
    }

    /// All attachments with the given name (case-insensitive).
    pub fn all_by_name(&self, name: &str) -> Vec<&Attachment> {
        let key = name.to_lowercase();
        self.attachments
            .values()
            .filter(|a| a.name.to_lowercase() == key)
            .collect()
    }

    /// All attachments in the index, in deterministic (path) order.
    pub fn attachments(&self) -> impl Iterator<Item = &Attachment> {
        let mut v: Vec<&Attachment> = self.attachments.values().collect();
        v.sort_by(|a, b| a.path.cmp(&b.path));
        v.into_iter()
    }

    /// All attachments of a given kind.
    pub fn by_kind(&self, kind: AttachmentKind) -> Vec<&Attachment> {
        self.attachments
            .values()
            .filter(|a| a.kind() == kind)
            .collect()
    }

    /// Total number of attachments indexed.
    pub fn len(&self) -> usize {
        self.attachments.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.attachments.is_empty()
    }
}

/// Best-effort MIME type from extension. Returns
/// `application/octet-stream` for unknown extensions.
pub fn mime_from_extension(ext: &str) -> String {
    let e = ext.trim_start_matches('.').to_ascii_lowercase();
    match e.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "m4v" => "video/x-m4v",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "oga" => "audio/ogg",
        "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        "opus" => "audio/opus",
        "aac" => "audio/aac",
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "odt" => "application/vnd.oasis.opendocument.text",
        "rtf" => "application/rtf",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "json" => "application/json",
        "yaml" | "yml" => "application/yaml",
        "toml" => "application/toml",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
    .to_string()
}

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

/// Suggest MIME type from a filename. Convenience for callers that
/// have just a string (e.g. resolving an embed reference like
/// `![[diagram.png]]`).
pub fn mime_from_filename(name: &str) -> String {
    let ext = Path::new(name)
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or("");
    mime_from_extension(ext)
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
        let dir = std::env::temp_dir().join(format!("tungsten-attach-{label}-{pid}-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(dir: &Path, rel: &str, body: &[u8]) -> PathBuf {
        let path = dir.join(rel);
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn mime_known_extensions() {
        assert_eq!(mime_from_extension("png"), "image/png");
        assert_eq!(mime_from_extension("PNG"), "image/png");
        assert_eq!(mime_from_extension(".pdf"), "application/pdf");
        assert_eq!(mime_from_extension("webm"), "video/webm");
        assert_eq!(mime_from_extension("mp3"), "audio/mpeg");
    }

    #[test]
    fn mime_unknown_falls_back() {
        assert_eq!(mime_from_extension("xyz"), "application/octet-stream");
        assert_eq!(mime_from_extension(""), "application/octet-stream");
    }

    #[test]
    fn kind_classification() {
        assert_eq!(AttachmentKind::from_extension("png"), AttachmentKind::Image);
        assert_eq!(AttachmentKind::from_extension("JPG"), AttachmentKind::Image);
        assert_eq!(AttachmentKind::from_extension("mp4"), AttachmentKind::Video);
        assert_eq!(AttachmentKind::from_extension("mp3"), AttachmentKind::Audio);
        assert_eq!(AttachmentKind::from_extension("pdf"), AttachmentKind::Pdf);
        assert_eq!(AttachmentKind::from_extension("docx"), AttachmentKind::Document);
        assert_eq!(AttachmentKind::from_extension("zip"), AttachmentKind::Other);
    }

    #[test]
    fn build_indexes_vault() {
        let dir = unique_vault("build");
        touch(&dir, "note.md", b"# Note\n"); // ignored
        touch(&dir, "img.png", &[0u8; 10]);
        touch(&dir, "doc.pdf", b"%PDF-1.4");
        touch(&dir, "audio.mp3", b"ID3");
        let idx = AttachmentIndex::build(&dir);
        assert_eq!(idx.len(), 3);
        let png = idx.by_name("img.png").unwrap();
        assert_eq!(png.kind(), AttachmentKind::Image);
        assert_eq!(png.mime_type, "image/png");
        assert_eq!(png.size_bytes, 10);
        let pdf = idx.by_name("doc.pdf").unwrap();
        assert_eq!(pdf.kind(), AttachmentKind::Pdf);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn build_skips_ignored_dirs() {
        let dir = unique_vault("ignored");
        touch(&dir, "real.png", b"x");
        touch(&dir, ".obsidian/file.png", b"x");
        touch(&dir, ".trash/old.png", b"x");
        let idx = AttachmentIndex::build(&dir);
        assert_eq!(idx.len(), 1);
        assert!(idx.by_name("real.png").is_some());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn build_skips_md_and_index_db() {
        let dir = unique_vault("skips");
        touch(&dir, "note.md", b"#");
        touch(&dir, "image.png", b"x");
        touch(&dir, ".tungsten/index.db", b"x");
        let idx = AttachmentIndex::build(&dir);
        assert_eq!(idx.len(), 1);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn by_name_case_insensitive() {
        let dir = unique_vault("case");
        touch(&dir, "Photo.PNG", b"x");
        let idx = AttachmentIndex::build(&dir);
        assert!(idx.by_name("photo.png").is_some());
        assert!(idx.by_name("PHOTO.PNG").is_some());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn by_kind_filters() {
        let dir = unique_vault("kind");
        touch(&dir, "a.png", b"x");
        touch(&dir, "b.png", b"x");
        touch(&dir, "c.pdf", b"x");
        touch(&dir, "d.mp3", b"x");
        let idx = AttachmentIndex::build(&dir);
        assert_eq!(idx.by_kind(AttachmentKind::Image).len(), 2);
        assert_eq!(idx.by_kind(AttachmentKind::Pdf).len(), 1);
        assert_eq!(idx.by_kind(AttachmentKind::Audio).len(), 1);
        assert_eq!(idx.by_kind(AttachmentKind::Video).len(), 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn all_by_name_returns_multiple() {
        let dir = unique_vault("dupes");
        touch(&dir, "folder1/diagram.png", b"x");
        touch(&dir, "folder2/diagram.png", b"y");
        let idx = AttachmentIndex::build(&dir);
        let results = idx.all_by_name("diagram.png");
        assert_eq!(results.len(), 2);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_picks_up_changes() {
        let dir = unique_vault("update");
        let path = touch(&dir, "test.png", &[0u8; 5]);
        let mut idx = AttachmentIndex::new();
        idx.update(&path);
        let a = idx.by_name("test.png").unwrap();
        assert_eq!(a.size_bytes, 5);
        fs::write(&path, &[0u8; 50]).unwrap();
        idx.update(&path);
        let a = idx.by_name("test.png").unwrap();
        assert_eq!(a.size_bytes, 50);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn remove_drops_attachment() {
        let dir = unique_vault("remove");
        let path = touch(&dir, "test.png", b"x");
        let mut idx = AttachmentIndex::new();
        idx.update(&path);
        assert_eq!(idx.len(), 1);
        assert!(idx.remove(&path));
        assert_eq!(idx.len(), 0);
        assert!(!idx.remove(&path));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mime_from_filename_handles_dots() {
        assert_eq!(mime_from_filename("diagram.png"), "image/png");
        assert_eq!(mime_from_filename("archive.tar.gz"), "application/octet-stream");
        assert_eq!(mime_from_filename("noext"), "application/octet-stream");
    }

    #[test]
    fn attachments_iterator_is_deterministic() {
        let dir = unique_vault("iter");
        touch(&dir, "z.png", b"x");
        touch(&dir, "a.png", b"x");
        touch(&dir, "m.png", b"x");
        let idx = AttachmentIndex::build(&dir);
        let names: Vec<&str> = idx.attachments().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["a.png", "m.png", "z.png"]);
        fs::remove_dir_all(&dir).ok();
    }
}
