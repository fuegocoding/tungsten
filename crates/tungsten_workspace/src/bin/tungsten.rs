//! `twctl` — umbrella command for vault maintenance and
//! diagnostics.
//!
//! Dispatches to the per-task subcommands. Run `twctl help`
//! to list them.
//!
//! The umbrella tries the `tungsten-*` binary in the same
//! directory as itself first, falling back to PATH. This
//! makes the workspace tree work without a global install.
//!
//! Subcommands:
//!     inspect       Vault summary (notes, links, tags, broken)
//!     outline       Heading outline for one or all notes
//!     switcher      Quick switcher (cmd-O) lookup
//!     graph         Force-directed layout of the link graph
//!     search        Free-text search across notes
//!     backlinks     Incoming links to a note
//!     broken        Unresolved wikilinks
//!     rename        Rename a note and rewrite its links
//!     find-broken   Alias for `broken`
//!     canvas        Parse / emit a JSON Canvas file
//!     publish       Render a note as HTML
//!     encrypt       Encrypt a file with Argon2id + XChaCha20
//!     decrypt       Decrypt a file
//!     plugins       List installed community plugins
//!     themes        List installed themes / base theme
//!     vault         Vault config dump
//!     index         Rebuild the SQLite index
//!     query         Run a DQL query
//!     init          Initialize a vault's sidecar state
//!     help          Print this list

use std::path::PathBuf;
use std::process::{Command, ExitCode};

const HELP: &str = "twctl — vault maintenance and diagnostics

USAGE:
    twctl <SUBCOMMAND> [ARGS]...

SUBCOMMANDS:
    inspect       Vault summary (notes, links, tags, broken)
    outline       Heading outline for one or all notes
    switcher      Quick switcher (cmd-O) lookup
    graph         Force-directed layout of the link graph
    graph-viz     Emit Graphviz DOT for the link graph
    search        Free-text search across notes
    backlinks     Incoming links to a note
    broken        Unresolved wikilinks
    rename        Rename a note and rewrite its links
    canvas        Parse / emit a JSON Canvas file
    publish       Render a note as HTML
    encrypt       Encrypt a file with Argon2id + XChaCha20
    decrypt       Decrypt a file
    plugins       List installed community plugins
    themes        List installed themes / base theme
    templates     List or render templates
    vault         Vault config dump
    index         Rebuild the SQLite index
    query         Run a DQL query
    shell         Interactive DQL REPL
    init          Initialize a vault's sidecar state
    sync          Encrypt vault state into a sync folder
    yip           Render a Year-in-Pixels SVG
    smart         List and evaluate smart folders
    export        Export a vault to a JSON document
    validate      Run pass/fail checks on a vault
    stats         Word counts and reading time
    diff          List notes changed in a date range
    doctor        Aggregate health report
    mood          Quick mood logger
    tasks         Aggregate open task markers
    graph-stats   Link graph statistics
    canvas-list   List every canvas file
    help          Print this list
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "help" || args[1] == "--help" || args[1] == "-h" {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let sub = &args[1];
    let sub_args: Vec<String> = args.iter().skip(2).cloned().collect();
    let program = match map_subcommand(sub) {
        Some(p) => p,
        None => {
            eprintln!("unknown subcommand: {sub}\n");
            eprint!("{HELP}");
            return ExitCode::from(2);
        }
    };
    let resolved = resolve_binary(program);
    let status = match Command::new(&resolved).args(&sub_args).status() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to run {}: {}", resolved.display(), e);
            return ExitCode::from(2);
        }
    };
    if status.success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(status.code().unwrap_or(1) as u8)
    }
}

/// Resolve a binary name. First try the same directory as
/// the running executable (so `cargo run -p
/// tungsten_workspace --bin twctl -- inspect ~/Notes`
/// works without `cargo install`). Then fall back to PATH.
fn resolve_binary(name: &str) -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return candidate;
            }
            let candidate_exe = dir.join(format!("{name}.exe"));
            if candidate_exe.is_file() {
                return candidate_exe;
            }
        }
    }
    PathBuf::from(name)
}

/// Map a subcommand name to the bin name we exec.
fn map_subcommand(sub: &str) -> Option<&'static str> {
    Some(match sub {
        "inspect" => "tungsten-inspect",
        "outline" => "tungsten-outline",
        "switcher" => "tungsten-switcher",
        "graph" => "tungsten-graph",
        "graph-viz" => "tungsten-graph-viz",
        "search" => "tungsten-grep",
        "backlinks" => "tungsten-backlinks",
        "broken" | "find-broken" => "tungsten-find-broken",
        "rename" => "tungsten-rename",
        "canvas" => "tungsten-canvas",
        "publish" => "tungsten-publish",
        "encrypt" => "tungsten-encrypt",
        "decrypt" => "tungsten-decrypt",
        "plugins" => "tungsten-plugins",
        "themes" => "tungsten-themes",
        "templates" => "tungsten-templates",
        "vault" => "tungsten-vault",
        "index" => "tungsten-index",
        "query" => "tungsten-query",
        "shell" => "tungsten-shell",
        "init" => "tungsten-init",
        "sync" => "tungsten-sync",
        "yip" => "tungsten-yip",
        "smart" => "tungsten-smart",
        "export" => "tungsten-export",
        "validate" => "tungsten-validate",
        "stats" => "tungsten-stats",
        "diff" => "tungsten-diff",
        "doctor" => "tungsten-doctor",
        "mood" => "tungsten-mood",
        "tasks" => "tungsten-tasks",
        "graph-stats" => "tungsten-graph-stats",
        "canvas-list" => "tungsten-canvas-list",
        _ => return None,
    })
}
