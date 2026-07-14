//! `tungsten-templates` — list and render templates for a
//! vault.
//!
//! Usage:
//!     tungsten-templates <VAULT_PATH> [NAME] [--title=T]
//!
//! With no `NAME`, lists all discovered templates:
//!     name<TAB>description
//!
//! With a `NAME`, prints the rendered template body to
//! stdout. Use `--title=...` to set the {{title}} var.
//!
//! Examples:
//!     tungsten-templates ~/Notes
//!     tungsten-templates ~/Notes daily
//!     tungsten-templates ~/Notes daily --title="My Day" > today.md

use std::path::PathBuf;
use std::process::ExitCode;

use chrono::Local;
use tungsten_workspace::{
    default_vars, discover_templates, render_template_body, vars_with_extras,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!(
            "usage: tungsten-templates <VAULT_PATH> [NAME] [--title=T]\n\
             \n\
             List templates (no NAME) or render one."
        );
        return ExitCode::from(2);
    }
    let vault = PathBuf::from(&args[1]);
    if !vault.is_dir() {
        eprintln!("not a directory: {}", vault.display());
        return ExitCode::from(2);
    }
    let registry = discover_templates(&vault);
    let mut title: Option<String> = None;
    for a in &args[2..] {
        if let Some(rest) = a.strip_prefix("--title=") {
            title = Some(rest.to_string());
        }
    }
    let name: Option<String> = args
        .get(2)
        .filter(|a| !a.starts_with("--"))
        .cloned();

    match name {
        None => {
            if registry.is_empty() {
                eprintln!("(no templates; create some in Templates/)");
                return ExitCode::SUCCESS;
            }
            for (name, t) in registry.iter() {
                let desc = t.description.as_deref().unwrap_or("");
                println!("{}\t{}", name, desc);
            }
        }
        Some(name) => {
            let Some(t) = registry.get(&name) else {
                eprintln!("no template named: {name}");
                return ExitCode::from(2);
            };
            let now = Local::now();
            let title_value = title.clone().unwrap_or_else(|| name.clone());
            let vars = vars_with_extras(
                now,
                title_value.clone(),
                std::iter::empty::<(String, String)>(),
            );
            let _ = default_vars(now, title_value);
            let out = render_template_body(&t.body, &vars);
            print!("{out}");
        }
    }
    ExitCode::SUCCESS
}
