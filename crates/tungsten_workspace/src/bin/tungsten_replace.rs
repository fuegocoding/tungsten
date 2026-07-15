//! `tungsten-replace` — batch find-and-replace across a vault.
//!
//! Usage:
//!     tungsten-replace <VAULT_PATH> <PATTERN> <REPLACEMENT>
//!         [--dry-run] [--glob=GLOB] [--include=EXT]
//!
//! Walks every note in the vault and applies a regex
//! replacement to each file's content. By default
//! modifies in place. With `--dry-run`, prints the count
//! of files that would change but does not write.
//! `--glob` and `--include` filter which files are touched
//! (default: every `.md` file).
//!
//! This is the same shape as Obsidian's "Find and Replace"
//! in bulk mode, with the addition of regex support.
//!
//! Examples:
//!     tungsten-replace ~/Notes "oldTag" "newTag" --dry-run
//!     tungsten-replace ~/Notes "\\[\\[Old" "[[New"
//!     tungsten-replace ~/Notes "Foo" "Bar" --glob="Journal/*.md"

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_workspace::NoteIndex;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-replace <VAULT> <PATTERN> <REPLACEMENT>\n\
             \n\
             Options:\n  \
               --dry-run       count changes without writing\n  \
               --glob=G        only touch files matching G (fnmatch)\n  \
               --include=E     only touch files with extension E (default: md)"
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let pattern = &args[2];
    let replacement = &args[3];
    let mut dry_run = false;
    let mut glob: Option<String> = None;
    let mut include: Option<String> = Some("md".into());
    for a in &args[4..] {
        if a == "--dry-run" {
            dry_run = true;
        } else if let Some(rest) = a.strip_prefix("--glob=") {
            glob = Some(rest.to_string());
        } else if let Some(rest) = a.strip_prefix("--include=") {
            include = Some(rest.to_string());
        }
    }

    let re = match regex::Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("regex error: {e}");
            return ExitCode::from(2);
        }
    };

    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(2);
        }
    };

    let mut changed = 0;
    let mut replacements = 0;
    for note in index.notes() {
        if let Some(ext) = &include {
            if note.path.extension().and_then(|s| s.to_str()) != Some(ext.as_str()) {
                continue;
            }
        }
        if let Some(g) = &glob {
            // fnmatch-style: we accept a simple `*` glob
            // over the relative path.
            let rel = note
                .path
                .strip_prefix(&vault)
                .unwrap_or(&note.path)
                .display()
                .to_string();
            if !glob_match(g, &rel) {
                continue;
            }
        }
        let new = re.replace_all(&note.content, replacement.as_str());
        if new == note.content {
            continue;
        }
        let count = re.find_iter(&note.content).count();
        replacements += count;
        if !dry_run {
            if let Err(e) = std::fs::write(&note.path, new.as_ref()) {
                eprintln!("write error on {}: {e}", note.path.display());
                return ExitCode::from(2);
            }
        }
        changed += 1;
    }
    eprintln!(
        "{} file(s) changed, {} replacement(s) made{}",
        changed,
        replacements,
        if dry_run { " (dry run)" } else { "" }
    );
    if changed == 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Simple `*` glob: matches a pattern containing `*` against
/// the input. Other characters are matched literally.
fn glob_match(pattern: &str, input: &str) -> bool {
    let mut pi = 0;
    let mut si = 0;
    let mut star_pi: Option<usize> = None;
    let mut star_si: usize = 0;
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = input.chars().collect();
    while si < s.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_pi = Some(pi);
            star_si = si;
            pi += 1;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            star_si += 1;
            si = star_si;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::glob_match;
    #[test]
    fn glob_basic() {
        assert!(glob_match("*.md", "Welcome.md"));
        assert!(glob_match("Journal/*.md", "Journal/2026-01-01.md"));
        assert!(!glob_match("*.md", "Welcome.markdown"));
        assert!(!glob_match("foo", "bar"));
    }
}
