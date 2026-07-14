//! Vault validation: pass/fail diagnostic checks.
//!
//! `tungsten-validate` runs a series of checks against a
//! vault and reports pass/fail with a short message. This
//! is the human-friendly counterpart to the verbose
//! `tungsten-inspect` report; it's also a CI-friendly
//! tool because the exit code is non-zero if any check
//! fails.
//!
//! Checks:
//!
//! - **frontmatter-yaml** — every note with a `---` opener
//!   has a parseable YAML frontmatter block
//! - **no-broken-links** — every wikilink/markdown link
//!   resolves to a known note by title
//! - **tags-non-empty** — notes with the `tags:` key have
//!   at least one tag
//! - **filename-unique** — no two notes share a stem
//!   (case-insensitive)
//! - **callout-kind-known** — every callout's `[!type]`
//!   is in the list of known kinds (`note`, `tip`, `warning`,
//!   `danger`, `info`, `example`, `question`, `success`,
//!   `failure`, `bug`, `quote`, `todo`)
//! - **link-target-not-self** — no note links to itself
//!   by title

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;

use crate::index::NoteIndex;
use crate::note::DEFAULT_CALLOUT_KINDS;
use crate::note_parser::parse;

/// One validation check.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub passed: bool,
    pub message: String,
    /// Number of items affected (notes, links, etc.).
    pub count: usize,
}

/// Overall validation report.
#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    pub passed: usize,
    pub failed: usize,
    pub checks: Vec<Check>,
}

impl ValidationReport {
    pub fn is_ok(&self) -> bool {
        self.failed == 0
    }
}

/// Run every check against `index`.
pub fn validate(index: &NoteIndex) -> ValidationReport {
    let mut checks = Vec::new();
    checks.push(check_frontmatter(index));
    checks.push(check_no_broken_links(index));
    checks.push(check_tags_non_empty(index));
    checks.push(check_filename_unique(index));
    checks.push(check_callout_kind_known(index));
    checks.push(check_no_self_link(index));

    let passed = checks.iter().filter(|c| c.passed).count();
    let failed = checks.len() - passed;
    ValidationReport {
        passed,
        failed,
        checks,
    }
}

fn check_frontmatter(index: &NoteIndex) -> Check {
    let mut bad = 0;
    for note in index.notes() {
        if note.content.starts_with("---") {
            // Parser would have set frontmatter; if it's
            // still Null, the YAML is unparseable.
            if matches!(note.frontmatter, serde_yaml::Value::Null) {
                bad += 1;
            }
        }
    }
    Check {
        name: "frontmatter-yaml".into(),
        passed: bad == 0,
        message: if bad == 0 {
            "all frontmatter blocks parse as YAML".into()
        } else {
            format!("{bad} note(s) have unparseable frontmatter")
        },
        count: bad,
    }
}

fn check_no_broken_links(index: &NoteIndex) -> Check {
    let mut broken = 0;
    for note in index.notes() {
        for link in &note.links {
            if index.by_title(&link.target).is_none() {
                broken += 1;
            }
        }
    }
    Check {
        name: "no-broken-links".into(),
        passed: broken == 0,
        message: if broken == 0 {
            "every link resolves to a known note".into()
        } else {
            format!("{broken} broken link(s) found")
        },
        count: broken,
    }
}

fn check_tags_non_empty(index: &NoteIndex) -> Check {
    let mut empty = 0;
    for note in index.notes() {
        if let Some(arr) = note.frontmatter.get("tags") {
            if let Some(seq) = arr.as_sequence() {
                if seq.is_empty() {
                    empty += 1;
                }
            } else if arr.as_str().map(|s| s.is_empty()).unwrap_or(false) {
                empty += 1;
            }
        }
    }
    Check {
        name: "tags-non-empty".into(),
        passed: empty == 0,
        message: if empty == 0 {
            "no note has an empty `tags:` list".into()
        } else {
            format!("{empty} note(s) have empty `tags:`")
        },
        count: empty,
    }
}

fn check_filename_unique(index: &NoteIndex) -> Check {
    let mut seen: std::collections::HashMap<String, i32> =
        std::collections::HashMap::new();
    for note in index.notes() {
        if let Some(stem) = note.path.file_stem().and_then(|s| s.to_str()) {
            let key = stem.to_ascii_lowercase();
            *seen.entry(key).or_insert(0) += 1;
        }
    }
    let dups: Vec<&String> = seen
        .iter()
        .filter(|(_, c)| **c > 1)
        .map(|(k, _)| k)
        .collect();
    Check {
        name: "filename-unique".into(),
        passed: dups.is_empty(),
        message: if dups.is_empty() {
            "all filenames are unique (case-insensitive)".into()
        } else {
            format!("{} duplicate filename(s)", dups.len())
        },
        count: dups.len(),
    }
}

fn check_callout_kind_known(index: &NoteIndex) -> Check {
    let known: BTreeSet<String> = DEFAULT_CALLOUT_KINDS
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut bad = 0;
    for note in index.notes() {
        for c in &note.callouts {
            if !known.contains(&c.kind) {
                bad += 1;
            }
        }
    }
    Check {
        name: "callout-kind-known".into(),
        passed: bad == 0,
        message: if bad == 0 {
            "every callout uses a known kind".into()
        } else {
            format!("{bad} callout(s) use an unknown kind")
        },
        count: bad,
    }
}

fn check_no_self_link(index: &NoteIndex) -> Check {
    let mut bad = 0;
    for note in index.notes() {
        for link in &note.links {
            if link.target.eq_ignore_ascii_case(&note.title) {
                bad += 1;
            }
        }
    }
    Check {
        name: "link-target-not-self".into(),
        passed: bad == 0,
        message: if bad == 0 {
            "no note links to itself".into()
        } else {
            format!("{bad} self-link(s) found")
        },
        count: bad,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> std::path::PathBuf {
        let base = std::env::temp_dir();
        let unique = format!(
            "tungsten-validate-test-{}-{}",
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
    fn empty_vault_passes() {
        let dir = tempdir();
        let index = NoteIndex::build(&dir).unwrap();
        let report = validate(&index);
        assert!(report.is_ok());
        assert_eq!(report.failed, 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn broken_link_fails_check() {
        let dir = tempdir();
        fs::write(dir.join("A.md"), "# A\nLinks to [[Missing]].\n").unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let report = validate(&index);
        let check = report
            .checks
            .iter()
            .find(|c| c.name == "no-broken-links")
            .unwrap();
        assert!(!check.passed);
        assert_eq!(check.count, 1);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn callout_unknown_kind_fails() {
        let dir = tempdir();
        fs::write(dir.join("A.md"), "# A\n> [!bogus] Hi\n").unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let report = validate(&index);
        let check = report
            .checks
            .iter()
            .find(|c| c.name == "callout-kind-known")
            .unwrap();
        assert!(!check.passed);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn callout_known_kind_passes() {
        let dir = tempdir();
        fs::write(dir.join("A.md"), "# A\n> [!note] Hi\n").unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let report = validate(&index);
        let check = report
            .checks
            .iter()
            .find(|c| c.name == "callout-kind-known")
            .unwrap();
        assert!(check.passed);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn self_link_fails() {
        let dir = tempdir();
        fs::write(dir.join("A.md"), "# A\nLinks to [[A]].\n").unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let report = validate(&index);
        let check = report
            .checks
            .iter()
            .find(|c| c.name == "link-target-not-self")
            .unwrap();
        assert!(!check.passed);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn duplicate_filename_fails() {
        let dir = tempdir();
        fs::create_dir_all(dir.join("subfolder")).unwrap();
        fs::write(dir.join("A.md"), "# A\n").unwrap();
        fs::write(dir.join("subfolder/a.md"), "# A\n").unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let report = validate(&index);
        let check = report
            .checks
            .iter()
            .find(|c| c.name == "filename-unique")
            .unwrap();
        assert!(!check.passed);
        fs::remove_dir_all(&dir).ok();
    }
}
