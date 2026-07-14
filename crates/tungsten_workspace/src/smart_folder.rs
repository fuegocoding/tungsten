//! Smart folders: saved searches as virtual folders.
//!
//! A "smart folder" is a DQL query that lives in a special
//! location under the vault (e.g. `.tungsten/smart/*.sf.md`)
//! and produces a dynamic list of notes. The query is
//! re-evaluated against the current index whenever the
//! folder is opened or the index is rebuilt.
//!
//! Smart folders are a lightweight, DQL-driven alternative
//! to Obsidian's "Saved Searches" plugin and to native
//! Bases. They compose with everything else: a smart
//! folder can use any DQL clause, so it's effectively
//! Turing-complete over the index.
//!
//! # File format
//!
//! A smart folder is a Markdown file with a `query:`
//! frontmatter key. Anything below the frontmatter is the
//! human-readable description:
//!
//! ```markdown
//! ---
//! query: LIST FROM #work WHERE file.mtime > "2026-01-01"
//! ---
//!
//! # Active projects
//!
//! Notes tagged \`#work\` modified since the start of the
//! year.
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use crate::dql::{execute, parse_query, DqlError, DqlResult, DqlRow};
use crate::index::NoteIndex;
use crate::note::Note;

/// A parsed smart folder: a query plus its human label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmartFolder {
    pub path: PathBuf,
    pub title: String,
    pub query: String,
}

impl SmartFolder {
    /// Run the smart folder's query against `index`,
    /// returning the rows.
    pub fn execute(&self, index: &NoteIndex) -> Result<DqlResult, DqlError> {
        let q = parse_query(&self.query)?;
        Ok(execute(&q, index))
    }
}

/// Discover every smart folder under
/// `<vault>/.tungsten/smart/` and `<vault>/.obsidian/smart/`.
/// Returns the parsed folders in sorted order.
pub fn discover(vault_root: &Path) -> Vec<SmartFolder> {
    let candidates = [
        vault_root.join(".tungsten").join("smart"),
        vault_root.join(".obsidian").join("smart"),
    ];
    let mut out = Vec::new();
    for dir in &candidates {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            if let Ok(sf) = parse(&path) {
                out.push(sf);
            }
        }
    }
    out.sort_by(|a, b| a.title.cmp(&b.title));
    out
}

/// Parse a single smart-folder file.
pub fn parse(path: &Path) -> Result<SmartFolder, std::io::Error> {
    let content = fs::read_to_string(path)?;
    let (frontmatter, body) = split_frontmatter(&content);
    let query = extract_query(&frontmatter).unwrap_or_default();
    let title = first_heading(&body)
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled")
                .to_string()
        });
    Ok(SmartFolder {
        path: path.to_path_buf(),
        title,
        query,
    })
}

fn split_frontmatter(content: &str) -> (String, String) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (String::new(), content.to_string());
    }
    let after_open = &trimmed[3..];
    let after_open = after_open.strip_prefix('\n').unwrap_or(after_open);
    if let Some(end_idx) = after_open.find("\n---") {
        let fm = after_open[..end_idx].to_string();
        let rest = &after_open[end_idx + 4..];
        let rest = rest.strip_prefix('\n').unwrap_or(rest);
        return (fm, rest.to_string());
    }
    (String::new(), content.to_string())
}

fn extract_query(frontmatter: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("query:") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

fn first_heading(body: &str) -> Option<String> {
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix('#') {
            let text = rest.trim_start_matches('#').trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

/// Evaluate every smart folder and pair each with its
/// result. Smart folders that fail to parse or
/// execute are returned as `Err` so the caller can show
/// a useful error in the UI.
pub fn evaluate_all(
    vault_root: &Path,
    index: &NoteIndex,
) -> Vec<(SmartFolder, Result<DqlResult, DqlError>)> {
    let folders = discover(vault_root);
    folders
        .into_iter()
        .map(|sf| {
            let result = sf.execute(index);
            (sf, result)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> std::path::PathBuf {
        let base = std::env::temp_dir();
        let unique = format!(
            "tungsten-smart-test-{}-{}",
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
    fn parse_smart_folder_with_query() {
        let dir = tempdir();
        let smart = dir.join(".tungsten/smart");
        fs::create_dir_all(&smart).unwrap();
        let p = smart.join("active.md");
        fs::write(
            &p,
            "---\nquery: LIST FROM #work\n---\n# Active work\n\nBody.\n",
        )
        .unwrap();
        let sf = parse(&p).unwrap();
        assert_eq!(sf.title, "Active work");
        assert_eq!(sf.query, "LIST FROM #work");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_uses_filename_when_no_heading() {
        let dir = tempdir();
        let smart = dir.join(".tungsten/smart");
        fs::create_dir_all(&smart).unwrap();
        let p = smart.join("no-heading.md");
        fs::write(&p, "---\nquery: LIST\n---\nNo heading here.\n").unwrap();
        let sf = parse(&p).unwrap();
        assert_eq!(sf.title, "no-heading");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_falls_back_when_no_frontmatter() {
        let dir = tempdir();
        let smart = dir.join(".tungsten/smart");
        fs::create_dir_all(&smart).unwrap();
        let p = smart.join("plain.md");
        fs::write(&p, "# Plain\n").unwrap();
        let sf = parse(&p).unwrap();
        assert_eq!(sf.title, "Plain");
        assert!(sf.query.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn discover_finds_smart_folders() {
        let dir = tempdir();
        let smart = dir.join(".tungsten/smart");
        fs::create_dir_all(&smart).unwrap();
        fs::write(
            smart.join("a.md"),
            "---\nquery: LIST\n---\n# A\n",
        )
        .unwrap();
        fs::write(
            smart.join("b.md"),
            "---\nquery: LIST\n---\n# B\n",
        )
        .unwrap();
        // non-md file should be skipped
        fs::write(smart.join("c.txt"), "x").unwrap();
        let folders = discover(&dir);
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].title, "A");
        assert_eq!(folders[1].title, "B");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn execute_runs_query() {
        let dir = tempdir();
        let j = dir.join("Journal");
        fs::create_dir_all(&j).unwrap();
        fs::write(j.join("2026-01-01.md"), "# A\n#work\n").unwrap();
        fs::write(j.join("2026-01-02.md"), "# B\n#work\n").unwrap();
        fs::write(j.join("2026-01-03.md"), "# C\n").unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let sf = SmartFolder {
            path: PathBuf::from("x"),
            title: "Work".into(),
            query: "LIST FROM #work".into(),
        };
        let rows = sf.execute(&index).unwrap();
        assert_eq!(rows.rows.len(), 2);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn evaluate_all_pairs_folders_with_results() {
        let dir = tempdir();
        let smart = dir.join(".tungsten/smart");
        fs::create_dir_all(&smart).unwrap();
        fs::write(
            smart.join("a.md"),
            "---\nquery: LIST FROM #work\n---\n# A\n",
        )
        .unwrap();
        fs::write(
            smart.join("b.md"),
            "---\nquery: LIST FROM #none\n---\n# B\n",
        )
        .unwrap();
        let j = dir.join("Journal");
        fs::create_dir_all(&j).unwrap();
        fs::write(j.join("2026-01-01.md"), "# A\n#work\n").unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let results = evaluate_all(&dir, &index);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0.title, "A");
        assert_eq!(results[0].1.as_ref().unwrap().rows.len(), 1);
        assert_eq!(results[1].0.title, "B");
        assert_eq!(results[1].1.as_ref().unwrap().rows.len(), 0);
        fs::remove_dir_all(&dir).ok();
    }
}
