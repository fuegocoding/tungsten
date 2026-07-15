//! `tungsten-demo` — create a small example vault for
//! learning and CI.
//!
//! Usage:
//!     tungsten-demo <DESTINATION>
//!
//! Creates a directory at the destination with a few
//! notes, a `.obsidian/` config, a `.tungsten/smart/`
//! folder, a `Journal/` with three days of entries, and
//! a `Templates/` folder. The vault is intentionally
//! small (≤20 notes) so it can be used to test any
//! Tungsten toolchain step.
//!
//! Examples:
//!     tungsten-demo /tmp/tungsten-demo
//!     tungsten-demo ~/Notes/demo

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-demo <DESTINATION>\n\
             \n\
             Create a small example vault at DESTINATION\n\
             with notes, config, smart folder, journal,\n\
             and templates."
        );
        return ExitCode::from(2);
    }
    let dest = PathBuf::from(&args[1]);
    if dest.exists() {
        eprintln!("destination already exists: {}", dest.display());
        return ExitCode::from(2);
    }
    if let Err(e) = std::fs::create_dir_all(&dest) {
        eprintln!("create error: {e}");
        return ExitCode::from(2);
    }
    // Layout
    let dirs = [
        "Subfolder",
        "Journal",
        "Templates",
        ".obsidian/themes",
        ".obsidian/plugins/alpha",
        ".tungsten/smart",
    ];
    for d in &dirs {
        if let Err(e) = std::fs::create_dir_all(dest.join(d)) {
            eprintln!("mkdir {d}: {e}");
            return ExitCode::from(2);
        }
    }
    // Notes
    let files: &[(&str, &str)] = &[
        (
            "Welcome.md",
            "# Welcome\n\nThis is the entry point of the demo vault.\n\n\
             See [[Subfolder/Sub-note]] for a linked sub-note, and [[Todo]] for an open task.\n",
        ),
        (
            "Todo.md",
            "---\ntag: inbox\n---\n# Todo\n\n- [ ] Pick a real task\n- [x] Try the demo\n",
        ),
        (
            "Subfolder/Sub-note.md",
            "# Sub-note\n\nA linked sub-page with [[Welcome]] back.\n",
        ),
        (
            "Templates/meeting.md",
            "# {{title}} on {{date}}\n\n## Attendees\n\n## Notes\n",
        ),
        (
            "Journal/2026-07-12.md",
            "## Mood\n\nmood: 7\nenergy: 6\n\n## Notes\n\nFirst journal entry.\n",
        ),
        (
            "Journal/2026-07-13.md",
            "## Mood\n\nmood: 8\nenergy: 7\n\n## Notes\n\nSecond entry.\n",
        ),
        (
            "Journal/2026-07-14.md",
            "## Mood\n\nmood: 6\nenergy: 5\n\n## Notes\n\nThird entry.\n\n> [!tip] Tip\n> Use the mood trend to see the pattern.\n",
        ),
        (
            ".obsidian/app.json",
            "{\"alwaysUpdateLinks\": true}\n",
        ),
        (
            ".obsidian/appearance.json",
            "{\"baseFontSize\": 16, \"theme\": \"moonstone\"}\n",
        ),
        (
            ".obsidian/community-plugins.json",
            "[\"alpha\"]\n",
        ),
        (
            ".obsidian/plugins/alpha/manifest.json",
            "{\"id\":\"alpha\",\"name\":\"Alpha\",\"version\":\"0.1.0\"}\n",
        ),
        (
            ".tungsten/smart/active.md",
            "---\nquery: LIST FROM #inbox\n---\n# Active inbox\n",
        ),
    ];
    for (path, body) in files {
        let p = dest.join(path);
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&p, body) {
            eprintln!("write {path}: {e}");
            return ExitCode::from(2);
        }
    }
    eprintln!(
        "demo vault created at {} ({} files)",
        dest.display(),
        files.len()
    );
    eprintln!("try: twctl doctor {}", dest.display());
    ExitCode::SUCCESS
}
