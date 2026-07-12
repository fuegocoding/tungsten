# Tungsten

> **A free, open-source, local-first personal knowledge application —
> built on Zed.**

[![License: GPL-3.0 OR Apache-2.0](https://img.shields.io/badge/license-GPL--3.0%20OR%20Apache--2.0-blue.svg)](LICENSE-GPL)
[![Substrate: Zed](https://img.shields.io/badge/substrate-zed--industries%2Fzed-orange.svg)](https://github.com/zed-industries/zed)

**Tungsten** is a FOSS spiritual successor to **Obsidian**, built by
forging the **Zed** editor (Rust + GPUI) and layering a
personal-knowledge-management stack on top: vault concept, Live Preview
Markdown, wikilinks, graph, Canvas, Bases, **Daily Journal** widgets, an
isolated **Obsidian compat subsystem** that runs unmodified Obsidian
plugins in a separate JS+DOM runtime, and (eventually) E2EE sync and
static Publish.

Drop-in compatibility with Obsidian vaults (`.obsidian/`) and a curated
**Daily Journal** differentiator with 12 templates, 20+ interactive
widgets, and a Year-in-Pixels view.

## Read first

- **[`PRD.md`](./PRD.md)** — what Tungsten is, the architecture, and the
  full Obsidian feature inventory.
- **[`ROADMAP.md`](./ROADMAP.md)** — milestones from fork to v1.0 launch
  (Q3 2027) and beyond.

## Architecture at a glance

```
+-----------------------------------------------------------------+
|  Tungsten                                                      |
|  +-----------------------------------------------------------+  |
|  |  Zed core (kept; Rust + GPUI + tree-sitter + LSP)         |  |
|  +-----------------------------------------------------------+  |
|  +-----------------------------------------------------------+  |
|  |  Tungsten note-taking layer (new, this repo's work)        |  |
|  |  tungsten_workspace, tungsten_markdown, tungsten_graph,   |  |
|  |  tungsten_journal, tungsten_canvas, tungsten_bases,        |  |
|  |  tungsten_theme, tungsten_eaar, tungsten_sync,             |  |
|  |  tungsten_publish                                          |  |
|  +-----------------------------------------------------------+  |
|  +-----------------------------------------------------------+  |
|  |  Obsidian compat subsystem (ISOLATED, separate process)    |  |
|  |  V8 + happy-dom + obsidian module shim + IPC to core       |  |
|  +-----------------------------------------------------------+  |
+-----------------------------------------------------------------+
```

The full architecture, design rationale, and tradeoffs are in
[`PRD.md` §4](./PRD.md#4-architecture-overview). The short version:
Zed gives us a fast, GPU-rendered, GPUI-native editor and a WASM
extension system; we add the note-taking layer and an isolated
Obsidian-compat subsystem so existing plugins keep working without
diluting the main extension architecture.

## Status

**v0.3 — Zed-based, pre-alpha.** The Zed fork is in place; rebranding,
CI, and the first native build are next. The v0.2 VS Code fork work was
discarded on 2026-07-12; see [`PRD.md` §2.1](./PRD.md#21-project-history-compressed).

The current phase-by-phase status is in the
[ROADMAP](./ROADMAP.md#quick-status--whats-done-vs-whats-left) header.

## Building from source

Tungsten is a fork of `zed-industries/zed`. The build instructions are
Zed's, with brand substitutions:

- **macOS:** [docs/src/development/macos.md](./docs/src/development/macos.md)
- **Linux:** [docs/src/development/linux.md](./docs/src/development/linux.md)
- **Windows:** [docs/src/development/windows.md](./docs/src/development/windows.md)

Quick Linux build:

```bash
# System deps (Debian/Ubuntu)
sudo apt install build-essential curl git libssl-dev libsqlite3-dev \
                 libfontconfig-dev libfreetype-dev libxkbcommon-dev \
                 libwayland-dev libvulkan1 mesa-vulkan-drivers

# Clone (if you haven't already) and run
git clone https://github.com/fuegocoding/tungsten.git
cd tungsten
./script/install
./script/run -d path/to/your/notes  # -d opens a directory
```

## Development workflow

This repository uses `cargo` and follows Zed's contribution conventions.
The repo's `AGENTS.md` (symlink to `.rules`) and individual crate
`.rules` files contain coding guidelines — read them before sending
patches.

Key paths:

- `crates/` — Zed's original crates (kept; upstream-trackable).
- `crates/tungsten_*/` — new Tungsten crates (added by us, in the same
  workspace).
- `extensions/tungsten-*/` — Tungsten native extensions (Rust, compiled
  to WASM).
- `extensions/obsidian-compat-*/` — Obsidian compat shims and bridges
  (TS/JS, isolated process).
- `docs/src/` — Zed's docs (kept; will be restyled and re-purposed for
  Tungsten).
- `script/` — Zed's build scripts (kept; brand substitutions only).
- `PRD.md`, `ROADMAP.md` — the source of truth for what we're building.

## License

Source code is dual-licensed under **GPL-3.0** OR **Apache-2.0** (at the
user's option), inherited from `zed-industries/zed`. See
[`LICENSE-GPL`](./LICENSE-GPL) and [`LICENSE-APACHE`](./LICENSE-APACHE)
for the full texts.

Native extensions written against the Tungsten Native Extension API may
use any license. Obsidian compat plugins keep their original license
terms.

## Contributing

See [`CONTRIBUTING.md`](./CONTRIBUTING.md) and
[`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md). All commits must be signed
(DCO).

## Sub-acknowledgement

Tungsten stands on the shoulders of:

- **[Zed](https://github.com/zed-industries/zed)** — the Rust + GPUI
  substrate this fork is built on. Without Zed, there is no Tungsten.
- **[Tree-sitter](https://github.com/tree-sitter/tree-sitter)** — Zed's
  parsing foundation; we add `tree-sitter-markdown` to it.
- **[Obsidian](https://obsidian.md)** — the product whose feature
  inventory defines our parity target.
- **[Foam](https://github.com/foambubble/foam)** — the original
  knowledge-management-on-a-text-editor inspiration. The v0.2
  Tungsten attempted to absorb Foam as a built-in module; in v0.3 the
  knowledge layer is re-implemented in Rust.
- **[Etebase](https://etebase.com)** — the E2EE sync protocol Tungsten
  uses by default.

— *Tungsten core team, 2026-07-12*
