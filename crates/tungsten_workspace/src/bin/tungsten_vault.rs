//! `tungsten-vault` — a small CLI for inspecting Tungsten vaults.
//!
//! Usage:
//!     tungsten-vault <PATH>
//!         Detect a vault at PATH (or above) and print its name,
//!         root, config dir, and a one-line summary of the loaded
//!         .obsidian/ config (theme, font size, accent color, plugin
//!         counts, snippet count, hotkey/plugin/theme catalog).
//!
//!     tungsten-vault --detect <PATH>
//!         Same as above but only walks up looking for .obsidian/;
//!         does not require the path to be a vault root.

use std::process::ExitCode;

use tungsten_workspace::{TungstenWorkspace, Vault};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args.len() > 3 || args[1] == "--help" || args[1] == "-h" {
        eprintln!("usage: tungsten-vault <PATH> | --detect <PATH>");
        return ExitCode::from(2);
    }

    let (detect_mode, path) = if args[1] == "--detect" {
        if args.len() != 3 {
            eprintln!("--detect requires a PATH");
            return ExitCode::from(2);
        }
        (true, std::path::PathBuf::from(&args[2]))
    } else {
        (false, std::path::PathBuf::from(&args[1]))
    };

    let vault: Option<Vault> = if detect_mode {
        Vault::detect(&path)
    } else {
        Vault::open(&path)
    };

    let Some(vault) = vault else {
        eprintln!("No vault found at or above {}", path.display());
        return ExitCode::from(1);
    };

    println!("Vault:           {}", vault.name());
    println!("Root:            {}", vault.root().display());
    println!("Config dir:      {}", vault.config_dir().display());

    let mut ws = TungstenWorkspace::new();
    if let Err(e) = ws.open_vault(vault, true) {
        eprintln!("Failed to register vault: {e}");
        return ExitCode::from(1);
    }

    let active_root = ws.active().unwrap().root().to_path_buf();
    match ws.config(&active_root) {
        Ok(cfg) => {
            println!();
            println!("appearance.json:");
            println!(
                "  cssTheme:        {}",
                cfg.appearance.css_theme.as_deref().unwrap_or("(none)")
            );
            println!(
                "  baseFontSize:    {}",
                cfg.appearance
                    .base_font_size
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "(default)".to_string())
            );
            println!(
                "  accentColor:     {}",
                cfg.appearance.accent_color.as_deref().unwrap_or("(default)")
            );
            println!(
                "  enabledSnippets: {}",
                cfg.appearance.enabled_css_snippets.len()
            );
            println!();
            println!("app.json:");
            println!(
                "  vimMode:         {}",
                cfg.app.vim_mode
            );
            println!(
                "  livePreview:     {}",
                cfg.app.live_preview
            );
            println!(
                "  alwaysUpdateLinks: {}",
                cfg.app.always_update_links
            );
            println!();
            println!("inventory:");
            println!("  themes:          {}", cfg.inventory.themes.len());
            for t in &cfg.inventory.themes {
                let display = t.display_name.as_deref().unwrap_or(&t.name);
                let version = t.version.as_deref().unwrap_or("?");
                println!("    - {display} (v{version})");
            }
            println!("  community plugins: {}", cfg.inventory.community_plugin_ids.len());
            for p in &cfg.inventory.plugins {
                let display = p.display_name.as_deref().unwrap_or(&p.id);
                let version = p.version.as_deref().unwrap_or("?");
                let enabled = if p.enabled { "enabled" } else { "disabled" };
                println!("    - {display} (v{version}, {enabled})");
            }
            println!("  snippets:        {}", cfg.inventory.snippets.len());
            for s in &cfg.inventory.snippets {
                println!("    - {s}");
            }
            println!("  core plugins:    {}", cfg.inventory.core_plugins.len());
        }
        Err(e) => {
            eprintln!("Failed to load .obsidian/ config: {e}");
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}
