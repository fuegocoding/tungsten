//! `tungsten-publish` — build a static HTML site from a vault.
//!
//! Usage:
//!     tungsten-publish <VAULT_PATH> <OUTPUT_DIR>
//!         Walk the vault, render every note to HTML, and write
//!         the result to <OUTPUT_DIR>/<note>.html. Attachment
//!         files (images, PDFs) are copied alongside.
//!
//!     tungsten-publish <VAULT_PATH> <OUTPUT_DIR> --no-attachments
//!         Render only .md -> .html; do not copy attachments.
//!
//! Output structure:
//!     <OUTPUT_DIR>/
//!       <Note Name>.html       one per note
//!       <Note Name>.md         (copy of the source for readers that
//!                                want raw markdown)
//!       images/, *.pdf, etc.   attachments (unless --no-attachments)
//!       index.html             a simple index linking every note
//!
//! Exit codes:
//!     0  success
//!     1  index error or I/O error
//!     2  bad arguments

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tungsten_workspace::{render_full_page, NoteIndex};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!(
            "usage: tungsten-publish [--no-attachments] <VAULT_PATH> <OUTPUT_DIR>\n\
             \n\
             Build a static HTML site from a vault. Every .md file\n\
             is rendered to <OUTPUT_DIR>/<name>.html; attachments\n\
             are copied alongside. An index.html is written that\n\
             links to every note."
        );
        return ExitCode::from(2);
    }
    let copy_attachments = !args.iter().any(|a| a == "--no-attachments");
    let positional: Vec<&String> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    if positional.len() != 2 {
        eprintln!("usage: tungsten-publish [--no-attachments] <VAULT_PATH> <OUTPUT_DIR>");
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(positional[0]);
    let out_dir = PathBuf::from(positional[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(1);
    }
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("create output dir failed: {e}");
        return ExitCode::from(1);
    }
    let index = match NoteIndex::build(&vault) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index error: {e}");
            return ExitCode::from(1);
        }
    };

    let mut notes_written = 0;
    let mut attachments_copied = 0;
    let mut index_links: Vec<String> = Vec::new();

    // 1. Render every note to HTML.
    for note in index.notes() {
        // Compute a relative path from the vault root for the
        // note, mirroring the vault's directory structure. This
        // way links between notes (relative paths) stay valid
        // when copied to the output dir.
        let rel = note
            .path
            .strip_prefix(vault.canonicalize().unwrap_or(vault.clone()))
            .unwrap_or(&note.path)
            .to_path_buf();
        // Replace .md with .html in the output filename. The
        // directory structure is preserved.
        let mut html_rel = rel.clone();
        html_rel.set_extension("html");
        let out_path = out_dir.join(&html_rel);
        if let Some(parent) = out_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("create dir {} failed: {e}", parent.display());
                return ExitCode::from(1);
            }
        }
        let html = render_full_page(note);
        if let Err(e) = std::fs::write(&out_path, html) {
            eprintln!("write {} failed: {e}", out_path.display());
            return ExitCode::from(1);
        }
        // Also copy the raw .md next to the .html for readers
        // who want the source. (Some publish workflows prefer to
        // skip this; a future --no-source flag could disable.)
        let md_out = out_path.with_extension("md");
        if let Err(e) = std::fs::copy(&note.path, &md_out) {
            eprintln!("copy {} failed: {e}", md_out.display());
        }
        notes_written += 1;
        index_links.push(format!(
            "<li><a href=\"{}\">{}</a></li>",
            html_rel.display().to_string().replace('\\', "/"),
            html_escape(&note.title)
        ));
    }

    // 2. Copy attachments.
    if copy_attachments {
        copy_attachments_recursive(&vault, &out_dir, &mut attachments_copied);
    }

    // 3. Write index.html.
    let mut links_html = String::new();
    for l in &index_links {
        links_html.push_str(l);
    }
    let index_html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Index</title>
<link rel="stylesheet" href="tungsten.css">
</head>
<body>
<article>
<h1>Index</h1>
<ul class="vault-index">
{links_html}
</ul>
</article>
</body>
</html>
"#
    );
    let index_path = out_dir.join("index.html");
    if let Err(e) = std::fs::write(&index_path, index_html) {
        eprintln!("write index.html failed: {e}");
        return ExitCode::from(1);
    }

    println!("Published {} note(s) to {}", notes_written, out_dir.display());
    if copy_attachments {
        println!("Copied {} attachment(s)", attachments_copied);
    }
    ExitCode::SUCCESS
}

fn copy_attachments_recursive(
    vault: &Path,
    out_dir: &Path,
    count: &mut usize,
) {
    use tungsten_workspace::attachments::AttachmentIndex;
    let index = AttachmentIndex::build(vault);
    for att in index.attachments() {
        // Mirror the relative path under the output dir.
        let rel = att
            .path
            .strip_prefix(vault.canonicalize().unwrap_or(vault.to_path_buf()))
            .unwrap_or(&att.path);
        let dest = out_dir.join(rel);
        if let Some(parent) = dest.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::copy(&att.path, &dest) {
            eprintln!("copy attachment {} failed: {e}", att.path.display());
            continue;
        }
        *count += 1;
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
