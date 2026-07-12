# Tungsten — Product Requirements Document

**Status:** Draft v0.3 — architecture pivot, Zed-based
**Last updated:** 2026-07-12
**Repository:** github.com/fuegocoding/tungsten (fork of zed-industries/zed)
**Authors:** Tungsten core team
**License:** GPL-3.0 OR Apache-2.0 (dual, inherited from Zed)

> **Major architecture change (v0.2 → v0.3).** Tungsten is no longer a fork of
> `microsoft/vscode`. It is a fork of `zed-industries/zed`, inheriting Zed's
> Rust + GPUI core, tree-sitter parsing, LSP/buffer/editor infrastructure, and
> GPU-accelerated rendering. The Obsidian-parity goal is unchanged; the
> implementation substrate changed. See §4 for the new architecture.

---

## 1. Executive Summary

**Tungsten** is a free and open-source, local-first personal knowledge application —
a fully FOSS spiritual successor to **Obsidian**, built by forking
**Zed** (the GPUI-based, Rust-native, GPU-rendered code editor) and layering a
personal-knowledge-management stack on top.

The goal is **drop-in compatibility with Obsidian** — vaults, plugins, themes,
workflow — while remaining entirely free, source-available, and free of
telemetry, brand-locked extensions, or proprietary cloud dependencies. The
core app is **free and 100% functional offline forever**. **Sync and Publish
are paid subscription services** (hosted), both **self-hostable for free** (so
the user is never locked in). E2EE sync uses the **Etebase** protocol (or
**Syncthing** for non-E2EE mesh sync); Publish is a static-site generator with
optional self-host.

Tungsten's **signature differentiator** is the **Daily Journal** feature
(§5.20): an opinionated, structured, widget-driven journaling experience that
is the same thing as the Obsidian daily note — the user does not switch
between two modes. The "daily note" core plugin is upgraded in-place with
interactive widgets (mood, energy, gratitude, habits, weather, etc.) and a
12-template curated library. The widget layer is **opt-out, not opt-in** — a
single toggle in Settings reverts the app to plain Obsidian-style daily notes
with zero behavior change. Data is plain YAML + Markdown, degrades gracefully
in vanilla Obsidian.

**Why Zed.** Zed is faster, lighter, and structurally cleaner than VS Code.
GPUI renders directly on the GPU; tree-sitter is a first-class dependency;
extensions are sandboxed WebAssembly modules; the entire editor is one
foreground thread with no renderer/extension-host split. This is the right
substrate for a note-taking app that will eventually need Canvas-style
infinite-zoom views, large-graph rendering, and real-time collaboration —
all of which a web-based stack handles poorly.

**Differentiator vs. the existing landscape:**

| App | Free? | Polished? | Plugin compat. | Encryption | Daily Journal? | Sync/Publish |
| --- | --- | --- | --- | --- | --- | --- |
| Obsidian | Free for personal use | ★★★★★ | Native | Optional, E2EE Sync | Templates only — no widgets | Paid sub (Sync + Publish) |
| Joplin | FOSS | ★★☆☆☆ (UX dated) | Partial | E2EE sync | No | Free, self-hostable |
| Logseq | FOSS | ★★★☆☆ | Partial | Local only | No (block-based) | Free |
| Day One | Paid (subscription) | ★★★★☆ | None | E2EE | Yes (its core) | Free sync |
| Zed + notes | FOSS | ★★★★☆ (target) | WASM | None | No | None |
| **Tungsten** | **FOSS core** | **★★★★★ (target)** | **Obsidian API (1:1, isolated compat subsystem)** + native WASM | **Local at-rest; E2EE sync via Etebase** | **Yes — 12 templates, 20+ widgets, opt-out** | **Paid sub (Etebase/Syncthing); self-hostable for free** |

---

## 2. Background and Motivation

### 2.1 Project history (compressed)

- **v0.1 (deprecated, archived 2026-07-12).** Tungsten was a fork of
  `microsoft/vscode@1.99.0`. ~50% of the shell was built
  (`extensions/tungsten-*` shipped 14 modules, `.obsidian/` loading worked,
  CI was green, first binary booted). The work is permanently discarded; this
  PRD is the only surviving artifact of that effort.

- **v0.2 (deprecated 2026-07-12).** Initial Zed-pivot draft of the PRD. Never
  implemented.

- **v0.3 (current).** This document. Active implementation.

### 2.2 Why Obsidian dominates but isn't enough

Obsidian (released 2020) became the default tool for personal knowledge
management (PKM) because it nailed the three things that matter:

1. **Local-first plain text** (Markdown in a folder — no lock-in).
2. **Wikilinks + backlinks** (the actual "second brain" UX).
3. **A real plugin ecosystem** (1,900+ community plugins, 250+ themes).

But Obsidian is **source-available, not FOSS**. The core app is proprietary;
only its plugin API is open. This means:

- No public security audit of the editor or sync engine.
- No community can fix bugs or backport patches.
- The publishing/sync services are paid SaaS.
- Restricted Mode exists because they don't trust the marketplace model enough.

### 2.3 Why existing FOSS alternatives fall short

- **Joplin** (2017) is FOSS and supports E2EE sync, but the UX is a notepad
  with a database — the *gestalt* of "live, linkable, navigable thought" is
  missing. No wikilink-aware autocompletion, graph view, canvas, or Live
  Preview.
- **Logseq** is a block-based outliner — different philosophy, less
  Obsidian-compatible.
- **Zed + notes** (what we are building) — Zed gives us a faster, lighter,
  GPUI-native editor. The note-taking layer, the knowledge-management
  layer, and the Obsidian compat layer are all new.

### 2.4 The thesis

By **forging Zed into a dedicated note-taking app** and **building the
personal-knowledge-management layer in-house**, we get:

- A battle-tested Rust+GPUI editor, buffer, tree-sitter parser, and LSP
  client. GPUI is the right UI framework for an app that will need infinite
  canvas, large graphs, and high-frequency redraws.
- A battle-tested WASM extension system for native Tungsten extensions.
- The ability to ship a **separate Obsidian compat subsystem** that runs
  unmodified Obsidian plugins in an isolated JS+DOM runtime, decoupled from
  the main extension path. (See §4.6.)
- A focused, opinionated shell that mimics Obsidian's UI down to the spacing,
  with the 70% of Zed that's noise for note-taking (debugger, terminal, Git
  panel, etc.) hidden by default but recoverable via Settings.

This is achievable. Zed is the substrate; the note-taking layer is the
product. We are building the layer.

---

## 3. Goals and Non-Goals

### 3.1 Goals (v1.0)

1. **Obsidian community plugin compatibility** — the Obsidian plugin API is
   a goal, but it is delivered via a **separate isolated subsystem** (§4.6),
   not by diluting the main extension architecture. The `obsidian` module's
   exports are re-implemented inside a JS+DOM runtime; plugins load into that
   runtime; Tungsten core talks to the runtime over IPC. The main extension
   system (Zed's WASM extensions, rebranded as Tungsten Native Extensions) is
   unaffected and is the recommended path for new plugins.
2. **Drop-in compatibility with Obsidian vaults** — open any existing
   `.obsidian/` vault; themes, plugins, hotkeys, and layouts from
   `.obsidian/` are respected. No conversion step.
3. **Polished UX matching Obsidian's text-driven experience** — Live Preview
   is the centerpiece. Markdown renders as you type on the focused line;
   the syntax (e.g. `#`) stays visible while you're editing it and fades only
   when you leave the line. No "switch to reading mode to see it pretty" —
   the edit surface is the read surface.
4. **FOSS, GPL-3.0 + Apache-2.0 dual-licensed core** — zero telemetry, zero
   proprietary cloud dependency for the core app. **Sync and Publish are
   paid subscription services** (because of cloud cost) but **self-hostable
   for free**. Users are never locked in.
5. **Local-first, offline-first, file-based** — your vault is a folder of
   plain Markdown. No required server, no required account, no required
   internet for the core app.
6. **Cross-platform** — Linux, macOS, Windows (desktop v1). Mobile is
   post-1.0.
7. **Superfast, no bloat** — Tungsten is a focused note app on Zed's GPUI
   substrate. Targets: cold-start < 500 ms; idle memory < 200 MB;
   keystroke-to-render < 4 ms (Zed-class).
8. **Encryption at rest (free, local, transparent)** — vaults can be stored
   as encrypted volumes using `age`/libsodium-backed encryption with a
   passphrase. Read/write is seamless; loss of passphrase = loss of data
   (intentional).
9. **E2EE sync (paid sub, self-hostable for free)** — uses the **Etebase**
   protocol (audited, open, used by Joplin and others). Alternative
   **Syncthing** mode for users who prefer a peer-to-peer mesh.
10. **Daily Journal as a first-class differentiator** — opinionated,
    widget-driven, template-rich journaling experience **merged into the
    daily note feature** (not a separate mode). 12 curated templates, 20+
    interactive widgets, Year-in-Pixels view, mood trend charts. A single
    Settings toggle ("Daily Journal features: on / off") reverts to plain
    Obsidian-style daily notes. See §5.20.

### 3.2 Non-Goals (v1.0)

- Mobile (iOS/Android) apps — post-1.0.
- Multiplayer / live collaborative editing — post-1.0.
- Cloud-hosted E2EE sync — post-1.0 (the self-hostable sync server can be
  built in v1.x).
- Replacing Zed for software development — Tungsten is a *different* app; it
  is not a "Zed with a note-taking mode." The note-taking shell hides most
  code-editor affordances by default.
- A web-based version of the full app — a Publish-style static-site generator
  is in scope; a live web editor is post-1.0.

### 3.3 Success metrics (v1.0)

- **≥ 80% of the top 100 Obsidian community plugins install and run
  unmodified in the isolated Obsidian compat subsystem.** Top 20 work
  flawlessly. (The remaining 20% require a quirk of the Chromium DOM that we
  polyfill imperfectly; plugin authors can fix incompatibilities on our
  issue tracker.)
- 100% of the 29 Obsidian core plugins have an equivalent in Tungsten.
- **Daily Journal: 12 curated templates ship at v1.0, ≥ 20 interactive
  widgets, Year-in-Pixels renders 1,825 cells (5 years) in < 50 ms.**
  Off-by-default toggle available in Settings.
- Cold-start time to a 1,000-note vault ≤ **500 ms** on M2 Air / Ryzen 5.
- Memory footprint ≤ **200 MB** idle.
- Keystroke-to-render latency ≤ **4 ms** (Zed-class Live Preview).
- Crash-free session rate ≥ 99.5%.
- All user data lives in the vault folder; deleting the app deletes zero
  data.

---

## 4. Architecture Overview

### 4.1 High-level architecture

```
+-----------------------------------------------------------------+
|                       Tungsten App                              |
|  +-----------------------------------------------------------+  |
|  |  Zed Core (kept as-is, rebranded as Tungsten shell)        |  |
|  |  - GPUI UI framework, GPU-accelerated rendering            |  |
|  |  - Buffer / tree-sitter / LSP / diagnostics / task system  |  |
|  |  - WASM extension host (Tungsten Native Extensions)        |  |
|  +-----------------------------------------------------------+  |
|  +-----------------------------------------------------------+  |
|  |  Tungsten Note-Taking Layer (new, this repo's work)        |  |
|  |  +-------------------+  +-------------------------------+  |  |
|  |  | tungsten_workspace|  | tungsten_markdown             |  |  |
|  |  |  - vault concept  |  |  - tree-sitter-markdown       |  |  |
|  |  |  - .obsidian/     |  |  - Live Preview decorations  |  |  |
|  |  |  - vault index    |  |  - wikilinks, callouts, etc.  |  |  |
|  |  +-------------------+  +-------------------------------+  |  |
|  |  +-------------------+  +-------------------------------+  |  |
|  |  | tungsten_graph    |  | tungsten_journal              |  |  |
|  |  |  - force-directed |  |  - Daily Journal widgets      |  |  |
|  |  |  - tags, orphans  |  |  - 12 templates               |  |  |
|  |  |  - link integrity |  |  - calendar, streak, YIP      |  |  |
|  |  +-------------------+  +-------------------------------+  |  |
|  |  +-------------------+  +-------------------------------+  |  |
|  |  | tungsten_canvas   |  | tungsten_bases                |  |  |
|  |  |  - JSON Canvas    |  |  - .base format               |  |  |
|  |  |  - GPUI infinite  |  |  - Table/List/Cards views     |  |  |
|  |  |    zoom surface   |  |  - filter/sort/formulas       |  |  |
|  |  +-------------------+  +-------------------------------+  |  |
|  |  +-------------------+  +-------------------------------+  |  |
|  |  | tungsten_theme    |  | tungsten_eaar                 |  |  |
|  |  |  - Obsidian CSS   |  |  - age + libsodium            |  |  |
|  |  |    variables      |  |  - Argon2id, XChaCha20-Poly1305| |  |
|  |  |  - snippets,      |  |  - transparent mount          |  |  |
|  |  |    cssclasses     |  |                               |  |  |
|  |  +-------------------+  +-------------------------------+  |  |
|  |  +-------------------+  +-------------------------------+  |  |
|  |  | tungsten_sync     |  | tungsten_publish              |  |  |
|  |  |  - Etebase (E2EE) |  |  - static site generator      |  |  |
|  |  |  - Syncthing      |  |  - local output folder        |  |  |
|  |  |  - self-hostable  |  |  - optional self-host server  |  |  |
|  |  +-------------------+  +-------------------------------+  |  |
|  +-----------------------------------------------------------+  |
|  +-----------------------------------------------------------+  |
|  |  Obsidian Compat Subsystem (ISOLATED, separate process)     |  |
|  |  - JS runtime: V8 (deno_core) or QuickJS                   |  |
|  |  - DOM polyfill: happy-dom or jsdom                        |  |
|  |  - Obsidian `app`, `Plugin`, `Vault`, `Workspace`, etc.    |  |
|  |  - Talks to Tungsten core over IPC (no shared memory)      |  |
|  |  - Can be disabled in Settings (zero cost when off)        |  |
|  +-----------------------------------------------------------+  |
+-----------------------------------------------------------------+
```

### 4.2 Why fork `zed-industries/zed` instead of staying on VS Code

The v0.2 VS Code fork produced a working shell but had structural problems
that the v0.3 Zed fork solves:

- **Performance.** VS Code is Electron: every window is a Chromium renderer
  process with a full DOM, a Node.js extension host, and a main process.
  Cold-start to a 1,000-note vault hit 2–3 s on M2 Air; idle memory was
  600+ MB. Zed is one foreground thread with a single GPUI window and a
  pool of background workers; we target 500 ms cold-start and 200 MB idle.
- **Renderer/host split.** VS Code's split (renderer = DOM, main = Node,
  extension host = Node) means every editor action crosses an IPC boundary
  and re-serializes DOM-shaped data. Zed has no such split; the buffer,
  tree-sitter, and editor live in the same process. Live Preview
  decorations are first-class.
- **GPU rendering.** GPUI renders directly on the GPU using Metal/Vulkan/
  DX12. List virtualization, large graph layouts, and infinite-zoom Canvas
  views all become tractable.
- **Tree-sitter as a first-class dependency.** Zed's grammar registry
  loads tree-sitter parsers as native libraries. We add tree-sitter-markdown
  (and tree-sitter-markdown-inline) and build Live Preview decorations on
  the parse tree.
- **WASM extensions.** Zed's extension system is sandboxed WASM. Native
  Tungsten extensions are WASM modules; they cannot touch the filesystem,
  the network, or arbitrary memory outside their linear-memory sandbox.

The cost: we lose 4 months of v0.2 work, switch the language from
TypeScript to Rust, and must build a separate Obsidian compat subsystem
(rather than getting "free" plugin loading via VS Code's extension host).
The user accepted that trade-off on 2026-07-12.

### 4.3 The Zed core we keep

We keep, unmodified except for branding and minor config:

- GPUI (`crates/gpui`) — UI framework, GPU rendering, flexbox layout, event
  loop, executor, context types (`App`, `Context<T>`, `Window`).
- The text buffer (`crates/buffer`) — Zed's gap-buffer, undo/redo, diff,
  transaction model.
- The editor (`crates/editor`) — Zed's editor view, multi-cursor, vim mode,
  selections, code-folding, completions, code actions, jump-to-definition.
- The tree-sitter registry (`crates/languages`, `crates/treesitter`) — every
  grammar, every parser integration. We add tree-sitter-markdown to this
  registry as a first-class language.
- The LSP client (`crates/lsp`) — used for Markdown language servers and
  for any plugin that wants LSP features.
- The workspace crate (`crates/workspace`) — multi-pane layout, panels,
  status bar, command palette, keybinding service. We extend, not replace.
- The extension host (`crates/extension_host`) — the WASM sandbox and
  extension API. Rebranded as Tungsten Native Extensions. We add new
  contribution points for note-taking features (§4.5).
- The settings UI (`crates/settings_ui`) — restyled to match Obsidian's
  look. We add a top-level "Vault" settings tab.
- The themes system (`crates/theme`) — **kept as Zed ships it**, including
  Zed's bundled themes (Gruvbox, One Dark, Solarized, etc.). Extended on
  top with Obsidian CSS variables for compatibility with community themes
  from `community-css-themes.json`. See §4.4.
- The terminal (`crates/terminal`) — **kept, advanced feature**. Hidden by
  default in the note-taking shell; user opts in via Settings → Advanced
  → Show terminal. See §4.4.
- The AI stack (`crates/agent`, `crates/copilot`, `crates/bedrock`,
  `crates/anthropic`, etc.) — **kept, advanced feature**. Available to
  users who want it; not surfaced in the default note-taking shell.
  Native extensions can register AI tools. See §4.4.
- The Agent Client Protocol (`crates/acp_thread`, `crates/acp_tools`) —
  **kept**. ACP is the standard way for agents (including AI assistants)
  to interact with the editor; Tungsten preserves it so existing agent
  integrations keep working.
- The collaboration client (`crates/collab`, `crates/client`) — unused at
  v1.0, kept for post-1.0 multiplayer.
- The file system watcher, the search crate, the git crate, the project
  panel — all kept. Some are surfaced in the note-taking shell (search,
  files); others (git, debug) are hidden by default and recoverable.

### 4.4 The Zed affordances we hide (not remove)

Zed is a code editor. Its default UI has affordances that are noise for a
note-taking app. We hide them by default but make them recoverable.
**Zed's own features — AI, ACP, terminal, themes, the extension system —
are not stripped; they are kept, available, and gated behind a single
"Advanced" toggle in Settings.**

| Affordance | Default in Tungsten | Recoverable via |
| --- | --- | --- |
| Integrated terminal | Hidden (advanced) | Settings → Advanced → Show terminal |
| Debugger | Hidden | Settings → Advanced → Show debugger |
| Git panel | Hidden | Settings → Advanced → Show Git panel |
| Run/Debug | Hidden | (Requires enabling terminal + debugger) |
| Tests | Hidden | Settings → Advanced → Show tests |
| Outline panel (code) | Replaced by tungsten_outline (headings) | (always visible) |
| File explorer | Renamed to "Files" and restyled | (always visible) |
| Project search | Renamed to "Find" with Obsidian operators | (always visible) |
| **AI / Copilot** | **Available, not promoted** | Settings → AI (Zed's existing settings) |
| **ACP (Agent Client Protocol)** | **Available** | Native extensions and external agents can attach via Zed's existing ACP surface |
| **Tungsten Native Extensions (WASM)** | **Available** | Settings → Plugins; same lifecycle as Zed extensions |
| **Zed themes (Gruvbox, One Dark, …)** | **Available** | Settings → Appearance → Theme (Zed's existing theme picker) |
| Code completions (LSP) | Off for `.md` files; on for code files in vault | Default |
| Diagnostics / Errors | Hidden for `.md`; on for code | Default |

The "Advanced" toggle is a single switch that reveals the terminal,
debugger, Git panel, and tests at once. The note-taking shell that the
user sees at first launch is intentionally sparse: left sidebar (Files,
Search, Bookmarks, Tags), right sidebar (Outline, Backlinks,
Properties), a centered editor, a status bar with backlinks-count,
word-count, mode, and a sync icon. But everything else is one toggle
away.

The note-taking shell that the user sees at first launch is intentionally
sparse: left sidebar (Files, Search, Bookmarks, Tags), right sidebar
(Outline, Backlinks, Properties), a centered editor, a status bar with
backlinks-count, word-count, mode, and a sync icon.

### 4.5 Tungsten Native Extensions (WASM, rebranded from Zed extensions)

Every new Tungsten feature that the user can disable is built as a
**Tungsten Native Extension** — a Rust crate compiled to WASM, loaded by
Zed's extension host, and toggled from Settings → Plugins. This includes:

- `tungsten-vault` — vault concept, `.obsidian/` loader, vault switcher
- `tungsten-markdown-editor` — Live Preview decorations on the buffer
- `tungsten-backlinks`, `tungsten-outline`, `tungsten-tags`,
  `tungsten-bookmarks`, `tungsten-properties` — sidebar panels
- `tungsten-templates`, `tungsten-daily-notes`, `tungsten-unique-note`
- `tungsten-graph` — force-directed graph view
- `tungsten-canvas` — JSON Canvas view
- `tungsten-bases` — `.base` view
- `tungsten-journal` — Daily Journal widgets
- `tungsten-search` — Obsidian search operators
- `tungsten-theme-obsidian` — Obsidian CSS variables and community themes
- `tungsten-eaar` — encryption at rest
- `tungsten-sync-etebase`, `tungsten-sync-syncthing` — sync transports
- `tungsten-publish` — static site generator
- `tungsten-importer` — Notion, Evernote, Roam, Keep, OneNote, Apple Notes
- `tungsten-cli` — headless command line

Native extensions are written in Rust, target `wasm32-wasip2`, and use the
Tungsten Native Extension API (a superset of Zed's extension API with new
contribution points: `registerJournalWidget`, `registerBasesView`,
`registerMarkdownPostProcessor`, `registerTheme`, `registerImporter`, etc.).

The native extension path is the **recommended** path for new plugins. It
is fast, GPUI-aware, sandboxed, and the API is small and stable.

### 4.6 The Obsidian Compat Subsystem (ISOLATED, separate process)

This is the trickiest part of the architecture, and the part the user
explicitly separated from the main extension system on 2026-07-12.

**Why a separate subsystem.** Obsidian's plugin API is TypeScript + DOM.
Obsidian plugins import from `obsidian`, use `document.createElement`,
`addEventListener`, and rely on a real browser DOM. Zed's extension host is
WASM with no DOM, no `window`, no `document`. Re-implementing the Obsidian
API on the native side would be a Sisyphean task and would create two
divergent APIs (Tungsten Native + a half-broken Tungsten Obsidian shim).

**Design.** The Obsidian Compat Subsystem is a **separate process** that:

1. Embeds a JavaScript engine (V8 via `deno_core`, or QuickJS — decision
   deferred to M5.1).
2. Embeds a DOM polyfill (`happy-dom` for fidelity, or a custom subset
   optimized for Obsidian plugins).
3. Re-implements the `obsidian` module's exports (`App`, `Plugin`,
   `Workspace`, `Vault`, `MarkdownView`, `Editor`, `MarkdownPostProcessor`,
   `Modal`, `Setting`, `Notice`, `EditorSuggest`, `FuzzySuggestModal`,
   `Component`, `PluginSettingTab`, etc.).
4. Loads Obsidian plugins (which are `.js`/`.ts` compiled to JS in their
   `main.js`) and runs them in the sandbox.
5. Talks to the Tungsten main process over a local IPC channel
   (Unix socket / Windows named pipe / macOS XPC). The IPC protocol mirrors
   the methods that the `obsidian` module would call into a real Obsidian
   app: `vault.read`, `vault.modify`, `workspace.getActiveFile`,
   `markdownView.editor.replaceRange`, etc.

The main Tungsten process treats the compat subsystem as a black box. It
launches the subsystem on demand (when the user enables Obsidian compat, or
when a community plugin is installed). The subsystem runs each plugin in
its own realm within the JS engine; plugins cannot access each other's
state except through documented APIs.

**Cost when off.** If the user disables "Obsidian plugin compat" in
Settings, the compat subsystem process is never launched. Zero runtime
overhead. The native extension system is unaffected.

**Tradeoffs.** The compat subsystem cannot match the perf of native
extensions (JS+polyfilled-DOM is slow compared to WASM). It also cannot
expose new GPUI-only features (like infinite-zoom canvas widgets) — those
are native-only. We accept this; the compat subsystem is for running
existing Obsidian plugins unmodified, not for building new features.

**Why "isolated".** The user's instruction was that Obsidian compat "can
be a different feature separated" from the main extension library. The
isolated process is the implementation of that separation. There is no
shared state, no shared memory, no shared extension API between native
extensions and Obsidian compat plugins. They are two distinct subsystems
that happen to live in the same app.

### 4.7 The Tungsten note-taking shell

Layered on top of Zed's `workspace` crate:

- **Left sidebar**: Files, Search, Bookmarks, Tags, Graph (icon), Canvas
  (icon), Sync status (icon), Settings (icon).
- **Right sidebar**: Outline, Backlinks, Properties, Daily Journal home
  (when in a journal entry).
- **Ribbon** (left, vertical, Obsidian-style): New note, Daily note,
  Random note, Graph, Canvas, Vault switcher, Sync, Settings.
- **Tab bar** (top): tab groups, stacked tabs, pop-out, pin.
- **Status bar** (bottom): backlinks count, mode (Source / Live / Reading),
  word count, sync status, current vault name.
- **Command palette** (`Cmd/Ctrl+Shift+P`): restyled with Tungsten commands
  prioritized.
- **Quick switcher** (`Cmd/Ctrl+O`): vault-wide, fuzzy.
- **Settings** (`Cmd/Ctrl+,`): a new top-level "Vault" tab alongside
  Appearance, Hotkeys, Plugins, About.

### 4.8 The Markdown engine

We add tree-sitter-markdown and tree-sitter-markdown-inline to Zed's
language registry. The buffer is parsed incrementally as the user types;
the parse tree drives Live Preview decorations.

- `tungsten_markdown::decorate(buffer, tree) -> Vec<Decoration>` runs on
  every buffer edit (debounced) and produces GPUI `Decoration` values
  (inline formatting, wikilink renames, code-block backgrounds, etc.).
- Source mode: zero decorations; the buffer renders as plain text.
- Live Preview mode: the focus-line rule — on the focused line, the
  Markdown source is visible; on every other line, decorations hide
  syntax markers. This is the Obsidian behavior we replicate.
- Reading mode: zero decorations that look like edit affordances; block
  elements (callouts, code blocks, math, mermaid) render fully.

### 4.9 The knowledge layer (built from scratch)

Tungsten's knowledge-management layer is built from scratch in Rust as
`tungsten_workspace` submodules. (Foam is not used, absorbed, or ported —
we rebuild the graph, tags, backlinks, and link-integrity logic directly
on top of `tungsten_workspace`.)

- Markdown parsing: links, tags, frontmatter, blocks, sections, callouts.
- `TungstenWorkspace`: a workspace of notes, indexed in memory, persisted
  to a sidecar SQLite DB (`vault/.tungsten/index.db`) for fast load.
- `TungstenGraph`: the link graph (nodes = notes, edges = wikilinks).
- Resource providers: Markdown + attachment.
- Note creation, templates, daily note, unique note.
- Query engine: DQL (Dataview Query Language) and a JS-flavored expression
  language (running on Boa or V8 — TBD).
- Link integrity: rename with wikilink rewriting.
- The whole layer is also accessible to native extensions via a public
  Rust API.

### 4.10 The Daily Journal (Tungsten differentiator)

See §5.20. Implementation: `tungsten_journal` crate + native extension
`tungsten-journal` (UI).

- `registerJournalWidget` API: native extensions register custom widget
  types (slider, scale, multi-select, number, text-area, photo, location,
  time-range, mood-board, habit-tracker, etc.).
- `journalWidgetExtension`: a buffer decoration that renders the YAML
  frontmatter as an interactive widget card on top of the closing `---`.
- Journal home panel: list of entries, calendar heatmap, streak counter,
  mood trend chart, Year-in-Pixels view.
- `Cmd/Ctrl+Shift+J` hotkey for "open today's entry."
- 12 built-in templates, all plain Markdown with YAML headers.

### 4.11 Other modules

- **Canvas** (`tungsten_canvas`): JSON Canvas reader + GPUI infinite-zoom
  view. Cards: text, note, media, web page, folder. Connections, groups,
  pan/zoom, embed in note.
- **Bases** (`tungsten_bases`): `.base` format reader. View types: Table,
  List, Cards, Map, Kanban, Calendar. Formulas, summaries, pluggable views
  via `registerBasesView`.
- **Theming** (`tungsten_theme`): all Obsidian CSS variables (light +
  dark). Community themes browser (download from
  `obsidian-releases/community-css-themes.json`). CSS snippets in
  `.obsidian/snippets/`. `cssclasses` property.
- **Encryption at rest** (`tungsten_eaar`): `age` + libsodium. Argon2id
  KDF. XChaCha20-Poly1305 AEAD. Transparent mount.
- **Sync** (`tungsten_sync`): Etebase (E2EE, default) + Syncthing (P2P,
  alternative). Self-hostable server.
- **Publish** (`tungsten_publish`): static site generator. Local output
  folder (free). Hosted (paid sub). Self-hostable server (free).

### 4.12 License

Zed is dual-licensed GPL-3.0 OR Apache-2.0. Tungsten inherits this. All
new code we write is contributed under the same dual-license. Plugins
written against the native extension API can be any license the author
chooses; plugins written against the Obsidian compat API inherit the
Obsidian plugin's license terms (which are author-chosen, but typically
MIT or Apache-2.0).

---

## 5. The Complete Obsidian Feature Inventory

This is the **1:1 parity target**. Every item here must be implemented (or
replaced with an equivalent) in Tungsten. Sources: `obsidian.md/help`,
`docs.obsidian.md`, `github.com/obsidianmd/obsidian-releases/community-plugins.json`
(1,900+ plugins), `github.com/obsidianmd/obsidian-help` (1.8k stars).

### 5.1 Vault & Files

| ID | Feature | Obsidian source | Tungsten implementation |
| --- | --- | --- | --- |
| V-01 | Vault = plain folder on disk | core | Plain `.md` files; no DB required at runtime |
| V-02 | Multi-vault support, vault switcher | core | `tungsten_workspace` + ribbon vault icon |
| V-03 | Per-vault config dir `.obsidian/` | core | Loader in `tungsten_workspace` |
| V-04 | Global settings dir | core | OS-standard app-data locations |
| V-05 | Accepted file types: `.md`, `.base`, `.canvas`, images, audio, video, PDFs | core | Full list, see §1.1 of research |
| V-06 | IndexedDB metadata cache (in-session) | core | Sidecar SQLite: `vault/.tungsten/index.db` |
| V-07 | File recovery (auto-snapshots) | core plugin #9 | 5-min default, 7-day retention, configurable |
| V-08 | File explorer with create/rename/move/delete | core plugin #8 | Zed's project panel, restyled |
| V-09 | Note composer (merge / extract) | core plugin #13 | Native extension |
| V-10 | Format converter (Roam/Bear/Zettelkasten) | core plugin #11 | Native extension |
| V-11 | Importer (Notion, Evernote, Roam, Keep, OneNote, Apple Notes, CSV) | community plugin (Obsidian-maintained) | `tungsten-importer` extension |
| V-12 | Export to PDF / Markdown / HTML | core | `tungsten_publish` (free for local output) |

### 5.2 Core Plugins (all 29, plus 2 maintained by Obsidian team)

The Obsidian core plugins. Each must be 1:1 in Tungsten:

| # | Plugin | Default | Tungsten implementation status |
| --- | --- | --- | --- |
| 1 | **Audio recorder** | on | Native — `cpal` for capture, opus encode via `audiopus` |
| 2 | **Backlinks** | on | `tungsten-graph` resolvedLinks + `tungsten-backlinks` panel |
| 3 | **Bases** | on (1.9+) | `tungsten-bases` extension — full `.base` format, 5 view types |
| 4 | **Bookmarks** | on | Native extension |
| 5 | **Canvas** | on | `tungsten-canvas` extension — JSON Canvas + GPUI view |
| 6 | **Command palette** | on | Zed's, restyled + extended |
| 7 | **Daily notes** | on | **Upgraded to Daily Journal** (see §5.20) |
| 8 | **File explorer** | on | Zed's project panel, restyled as "Files" |
| 9 | **File recovery** | on | `tungsten_workspace` snapshot store |
| 10 | **Footnotes view** | on | Native extension |
| 11 | **Format converter** | on | Native extension |
| 12 | **Graph view** | on | `tungsten-graph` package, force-directed on GPUI |
| 13 | **Note composer** | on | Native extension |
| 14 | **Outgoing links** | on | Native extension |
| 15 | **Outline** | on | Native extension (headings) |
| 16 | **Page preview** | on | Native extension (hover popover) |
| 17 | **Properties view** | on | Native extension |
| 18 | **Publish** | on | `tungsten-publish` (free local, paid hosted) |
| 19 | **Quick switcher** | on | Zed's Quick Open, extended |
| 20 | **Random note** | on | Trivial |
| 21 | **Search** | on | Native extension with Obsidian operators |
| 22 | **Slash commands** | on | Buffer decoration + keybinding |
| 23 | **Slides** | on | Native extension (presenting mode) |
| 24 | **Sync** | on | **Paid add-on, post-1.0.** Free: local vaults only |
| 25 | **Tags view** | on | `tungsten-graph` tag index + panel |
| 26 | **Templates** | on | `tungsten-workspace` template engine |
| 27 | **Unique note creator** | on | `tungsten-workspace` unique note |
| 28 | **Web viewer** | on | Native — Zed's webview crate (CEF/webkit2gtk) |
| 29 | **Word count** | on | Status bar (always-on) |
| 30 | **Workspaces** | on | Zed's, repackaged |
| 31 | **Tab groups, stacked tabs, pop-out, linked views** | core (not a plugin) | Part of `tungsten-workspace` shell |

### 5.3 Editor & Live Preview (tree-sitter-markdown decorations)

| ID | Feature | Notes |
| --- | --- | --- |
| E-01 | Soft line breaks (Shift+Enter = `<br>`) | |
| E-02 | Strict line breaks (toggle) | |
| E-03 | Headings `#`–`######` with autocomplete | |
| E-04 | Bold/italic/strikethrough/highlight inline rendering | |
| E-05 | Internal links (wikilink + markdown) | |
| E-06 | External links with autocompletion | |
| E-07 | Image embeds with size hints (`|WxH`) | |
| E-08 | Quotes, lists (ordered/unordered/task), nesting | |
| E-09 | Task list checkboxes (any char as done marker) | |
| E-10 | Horizontal rules | |
| E-11 | Inline code + fenced code (tree-sitter syntax highlighting) | |
| E-12 | Footnotes (`[^id]`, inline `^[text]`) | |
| E-13 | Comments `%% ... %%` (editing view only) | |
| E-14 | Backslash escaping | |
| E-15 | Tables with column alignment, right-click sort | |
| E-16 | Mermaid diagrams in code blocks | |
| E-17 | MathJax inline `$..$` and block `$$..$$` | |
| E-18 | Block references `^id` and `[[note#^id]]` | |
| E-19 | Note embeds `![[file]]` (notes, images, audio, PDF with `#page=N` `#height=N`, canvas) | |
| E-20 | Callouts `> [!type]` (21 default types, foldable, nestable, custom via CSS) | |
| E-21 | Properties (YAML frontmatter) — see §5.4 | |
| E-22 | HTML comments, raw HTML, `<iframe>` embeds | |
| E-23 | Find / Replace in file | |
| E-24 | Spell check (OS-native) | |
| E-25 | Vim mode (built-in option) | Inherited from Zed |
| E-26 | Drag-and-drop headings in outline | |
| E-27 | Slash command palette `/` | |
| E-28 | Drag-drop of files (creates wikilinks/embeds) | |
| E-29 | Multi-cursor, column selection, code-folding | Inherited from Zed |

### 5.4 Properties (YAML frontmatter)

| ID | Feature |
| --- | --- |
| P-01 | Default property keys: `tags`, `aliases`, `cssclasses` |
| P-02 | Publish property keys: `publish`, `permalink`, `description`, `image`, `cover` |
| P-03 | Property types: Text, List, Number, Checkbox, Date, Date&Time, Tags |
| P-04 | Property display modes: Visible, Hidden, Source |
| P-05 | Hotkey: `Cmd/Ctrl+;` adds a property |
| P-06 | JSON syntax (parsed & re-saved as YAML) |
| P-07 | Property-based search `[prop]`, `[prop:value]`, `null`, `(...)`, `OR`, regex, double-quotes |
| P-08 | "All properties" vault-wide panel (sortable by name/frequency) |
| P-09 | Per-property type icons |
| P-10 | Deprecation migrations (`alias` → `aliases`, etc.) via Format converter |

### 5.5 Markdown: Internal Links & Embeds

| ID | Feature |
| --- | --- |
| L-01 | Wikilink `[[Name]]` |
| L-02 | Wikilink to heading `[[Name#Heading]]` |
| L-03 | Wikilink to block `[[Name#^id]]` (human-readable IDs) |
| L-04 | Wikilink alias `[[Name\|display text]]` |
| L-05 | Wikilink to section `[[Name##header]]` |
| L-06 | Wikilink sub-heading `[[#A#B]]` |
| L-07 | Wikilink autocompletion on `[[` (fuzzy, with alias support) |
| L-08 | Wikilink creation on non-existent note ("Create new note" prompt) |
| L-09 | Unlinked mentions detection & conversion |
| L-10 | Embed notes `![[Name]]` |
| L-11 | Embed sections, blocks, PDF page+height, canvas (shapes only), list (via block ID), search (`query` code block) |
| L-12 | Link-preview on hover (Page preview plugin) |
| L-13 | Link rewriting on file rename |
| L-14 | Resolved vs. unresolved links (the "graph" data) |
| L-15 | MD link `[text](path.md)` (relative MD links supported) |

### 5.6 Search

| ID | Feature | Notes |
| --- | --- | --- |
| S-01 | `Ctrl/Cmd+Shift+F` opens Search | |
| S-02 | Search operators: `file:`, `path:`, `content:`, `match-case:`, `ignore-case:`, `tag:`, `line:`, `block:`, `section:`, `task:`, `task-todo:`, `task-done:` | |
| S-03 | Boolean (`AND` default, `OR`, `-NOT`) | |
| S-04 | Property search `[prop]`, `[prop:value]`, `null` | |
| S-05 | Regex (`/.../`) | |
| S-06 | Sort: file name, modified, created (asc/desc) | |
| S-07 | Copy results | |
| S-08 | Explain search term | |
| S-09 | Embed via `query` code block in note | |
| S-10 | Recently-changed search history | |
| S-11 | Search within canvas files | |

### 5.7 Graph View

| ID | Feature |
| --- | --- |
| G-01 | Force-directed graph rendering of all notes |
| G-02 | Filters: search, tags, attachments, existing-files-only, orphans |
| G-03 | Groups: named colored groups via search terms |
| G-04 | Display: arrows, text fade threshold, node size, link thickness, animate |
| G-05 | Forces: center, repel, link, link distance |
| G-06 | Time-lapse animation by creation time |
| G-07 | Local graph (per-note, in sidebar) |
| G-08 | Multiple named graph views (saved filter+style) |
| G-09 | Click-to-open, hover-to-highlight |
| G-10 | Color by tag, folder, type |
| G-11 | Performance: 10k+ notes without dropping frames |

### 5.8 Canvas

| ID | Feature |
| --- | --- |
| C-01 | `.canvas` file extension, JSON Canvas open format |
| C-02 | Cards: text, note, media (image/audio/video/PDF), web page, folder |
| C-03 | Connections: directed, labeled, colorable |
| C-04 | Groups: empty or from selection |
| C-05 | Pan/zoom: space-drag, middle-mouse, Ctrl/Cmd+scroll |
| C-06 | Fit selection, fit all, reset |
| C-07 | Embed in note: `![[My.canvas]]` (shows shapes only) |
| C-08 | Right-click connection → Go to source/target |
| C-09 | Search-within-canvas |
| C-10 | Drag-drop attachments and folders |

### 5.9 Bases (database-like views)

| ID | Feature |
| --- | --- |
| B-01 | `.base` files (plain JSON) |
| B-02 | View types: Table, List, Cards, Map (built-in) |
| B-03 | Pluggable views via `registerBasesView` |
| B-04 | Filter / sort / group by properties |
| B-05 | Formulas with Functions library |
| B-06 | Summaries (totals, averages, counts) |
| B-07 | Embed via `base` code block in note |
| B-08 | Inline note-editing within bases |

### 5.10 Tags

| ID | Feature |
| --- | --- |
| T-01 | Tags via frontmatter `tags:` or inline `#tag` |
| T-02 | Tag Explorer panel (hierarchical) |
| T-03 | Nested tags (e.g. `#parent/child`) |
| T-04 | Click tag → search; Ctrl/Cmd-click → add to search |
| T-05 | Sort by name or frequency; tree or flat |
| T-06 | Tag rename refactoring |
| T-07 | Tag completion in editor |

### 5.11 Bookmarks

| ID | Feature |
| --- | --- |
| BK-01 | Bookmark: files, folders, graphs, searches, headings, blocks, links (web viewer) |
| BK-02 | Bookmark groups (folders), reorderable |
| BK-03 | Pin to panel |
| BK-04 | Multi-select in file explorer with Alt/Shift-click |
| BK-05 | Web viewer address bar bookmark button |

### 5.12 Tabs, Windows, Sidebar, Ribbon, Status Bar

| ID | Feature |
| --- | --- |
| TB-01 | New tab (`Ctrl/Cmd+T` or "+") |
| TB-02 | Open in new tab (`Ctrl/Cmd+click`, +Shift in Source mode) |
| TB-03 | New tab group (`Ctrl/Cmd+Alt+click`) |
| TB-04 | New window (`Ctrl/Cmd+Alt+Shift+click`) |
| TB-05 | Split right/down, drag-and-drop |
| TB-06 | Stacked tabs (Andy Matuschak sliding notes) |
| TB-07 | Linked views (Graph local, Backlinks, Outline auto-update) |
| TB-08 | Pop-out windows (drag tab out, or "Move to new window") |
| TB-09 | Tab switching: `Ctrl+Tab`, `Ctrl+1..8`, `Ctrl+9` (last), `Ctrl+Shift+T` (recently closed) |
| TB-10 | Pinned tabs (main editor + sidebar) |
| TB-11 | Right-click tab menu (Pin, Split, Close, Move, Bookmark) |
| SB-01 | Left + right sidebars on desktop, swipe on mobile |
| SB-02 | Tab groups in sidebar |
| SB-03 | Pinned tabs in sidebar (notes stay put, panes stay focused) |
| SB-04 | Per-tab icons; drag-drop between groups |
| SB-05 | Drag note into sidebar to keep visible |
| RB-01 | Left ribbon, always visible on desktop |
| RB-02 | Customize: right-click uncheck, drag to reorder |
| RB-03 | Mobile: bottom-right menu, configurable quick-access |
| RB-04 | Hide via Settings → Appearance → Advanced |
| SS-01 | Backlinks count, mode (source/live preview/reading), sync status, word count |
| SS-02 | Click sync icon → sync log |
| SS-03 | Items added by plugins (custom status bar items) |

### 5.13 Hotkeys & Customization

| ID | Feature |
| --- | --- |
| H-01 | Fully user-customizable per-command |
| H-02 | Multiple hotkeys per command |
| H-03 | Filter "only assigned" |
| H-04 | Non-US keyboard layouts supported |
| H-05 | Editing shortcuts are OS-level and not customizable (separately documented) |

### 5.14 Themes & CSS

| ID | Feature |
| --- | --- |
| TH-01 | Community themes from `obsidian-releases/community-css-themes.json` |
| TH-02 | Browse/install/update/uninstall via Settings → Appearance → Themes |
| TH-03 | All Obsidian CSS variables implemented (light + dark) |
| TH-04 | CSS snippets in `.obsidian/snippets/` |
| TH-05 | `cssclasses` property per-note |
| TH-06 | Custom callouts via CSS (`--callout-color`, `--callout-icon` with Lucide/SVG) |
| TH-07 | Style Inspector workflow |
| TH-08 | Light/dark/auto mode |

### 5.15 Sync (paid subscription; self-hostable for free)

> **Why paid?** E2EE sync incurs cloud storage, egress, and database costs.
> Self-hosting is free forever — you run your own server. Hosted sync is a
> paid sub so we can pay for the infrastructure and the third-party crypto
> audit.

| ID | Feature | Notes |
| --- | --- | --- |
| SY-01 | **Etebase (EteSync) protocol** | Default E2EE transport. Library: `etebase-py` / `etebase-server` (Apache-2.0 / AGPL-3.0). |
| SY-02 | **Syncthing mode** | P2P, no server. Folder-level replication. |
| SY-03 | End-to-end encryption (default) | Server stores ciphertext only. |
| SY-04 | Standard encryption (server has key) | For shared vaults only. |
| SY-05 | Cipher: XChaCha20-Poly1305, Argon2id KDF | |
| SY-06 | Multi-region hosted (US, EU, APAC) | |
| SY-07 | **Self-hostable server** | Docker image + Helm chart. |
| SY-08 | Shared vaults for collaboration | |
| SY-09 | Version history (1 year default on hosted; configurable on self-hosted) | |
| SY-10 | Sync log/status icon in status bar | |
| SY-11 | Selective sync (per-folder) | |
| SY-12 | Vault-config sync (themes, plugins, hotkeys) | |
| SY-13 | Headless CLI for sync scripting | |
| SY-14 | Third-party crypto audit | cure53-style audit on the Tungsten client + self-hostable server, before public launch. |
| SY-15 | **Onboarding: bring your own Etebase server** | A user with an existing Etebase/Joplin server can point Tungsten at it. No vendor lock-in. |

**Pricing (hosted):**

- **Free:** local vaults, EaaR, all core + community plugins, static Publish, no Sync.
- **Pro ($5/mo):** Etebase-hosted sync, 1-year history, 10 GB. Cancel anytime; local vault still works.
- **Team ($12/user/mo):** shared vaults, audit log, SSO (OIDC), 1-year history.

**Self-hosted: free forever** (you pay only for your own VPS / S3 bucket).

### 5.16 Publish (paid subscription for hosted; self-hostable for free)

| ID | Feature | Tier |
| --- | --- | --- |
| PU-01 | Static site generator for selected notes | Core (free) + hosted |
| PU-02 | **Local output folder** (no server required) | **Core (free)** — `tungsten publish` outputs to a folder |
| PU-03 | **Hosted Publish** (one-click deploy to `your-site.tungsten.md`) | Pro/Team sub |
| PU-04 | Custom domains | Pro/Team sub |
| PU-05 | Password-protected sites | Pro/Team sub |
| PU-06 | Full-text search on site | Pro/Team sub |
| PU-07 | Permalinks, SEO meta | Pro/Team sub |
| PU-08 | Social media link previews | Pro/Team sub |
| PU-09 | Publish directory `publish/your-site/` | Hosted |
| PU-10 | Optional self-hosted "Tungsten Publish" server | **Free (self-hostable)** |

### 5.17 Web Clipper (browser extension, post-1.0)

| ID | Feature |
| --- | --- |
| WC-01 | Browser extension (Chrome, Firefox, Safari) |
| WC-02 | Highlights & save web content to vault |
| WC-03 | Templates with logic (conditionals, loops, fallbacks) |
| WC-04 | Interpreter (NL content extraction) |
| WC-05 | Reader view (distraction-free) |
| WC-06 | Send to vault via end-to-end encrypted channel |

### 5.18 Mobile (iOS + Android, post-1.0)

| ID | Feature |
| --- | --- |
| MB-01 | iOS + Android native (or React Native for v1) |
| MB-02 | Local-only mode (no sync required) |
| MB-03 | Touch-optimized UI refresh |
| MB-04 | Mobile widgets (lock screen, home screen quick actions) |
| MB-05 | Siri / Shortcuts integration |
| MB-06 | Tab switcher |
| MB-07 | CSS snippets support |
| MB-08 | Mobile file recovery (subject to OS) |

### 5.19 Other Features

| ID | Feature |
| --- | --- |
| O-01 | Obsidian URI scheme (`obsidian://`) — Tungsten will register a parallel `tungsten://` but also accept `obsidian://` for compat. |
| O-02 | Command-line interface (`tungsten` binary). |
| O-03 | Keychain / Secret Storage — plugins can store secrets via `app.secretStorage`. |
| O-04 | Right-to-left (RTL) UI |
| O-05 | Restricted Mode (sandboxed plugin mode) |
| O-06 | Hot-reload plugin dev |
| O-07 | Accessibility: full keyboard nav, screen reader, high-contrast theme |
| O-08 | Internationalization: 10+ languages (en, ar, de, es, fr, it, ja, ko, pt-BR, ru, zh-CN) |
| O-09 | Tables visual editor (right-click context menu) |
| O-10 | Formatting menu (right-click context menu for inline formatting) |

### 5.20 Daily Journal (Tungsten differentiator — *upgraded Daily Notes*)

**The daily note IS the daily journal.** Tungsten does not have a separate
"Daily Journal" feature and a separate "Daily Notes" feature — those would
be the same thing with two UIs. Instead, Obsidian's "Daily Notes" core
plugin is upgraded in-place: the file format and the user's mental model
are unchanged, but the editor experience is enriched with optional
interactive widgets, calendar/heatmap visualization, and a curated template
library. A single Settings toggle disables the entire enrichment layer and
reverts to plain Obsidian-style daily notes with zero behavior change.

This is the **single biggest differentiator vs Obsidian** in v1.0. It is
designed to make Tungsten the default app for users who want a real
journaling practice (not just a notebook), without sacrificing the
free-form Markdown that PKM users love, and without breaking 1:1
compatibility with existing Obsidian daily-note vaults.

#### 5.20.1 Design principles

1. **Same file format, same hotkey, same folder.** `Ctrl/Cmd+Shift+J`
   opens today's file at `Journal/2026-07-08.md`. The Daily Notes core
   plugin's settings are reused as-is.
2. **Text-driven experience first.** Free-form Markdown body is the
   centerpiece. Widgets are a thin layer at the top — they never
   obstruct the writing flow. The user types; widgets update; the cursor
   stays in the writing flow.
3. **Plain Markdown under the hood.** Every entry is a `.md` file. No
   proprietary database; drop the folder into Obsidian and it works.
4. **Widgets are stored as YAML frontmatter + a renderer.** A mood slider
   is just a number (`mood: 7`) in frontmatter; the renderer draws the
   interactive UI on top of the YAML in Live Preview. Source mode shows
   the raw YAML.
5. **Templates are first-class and text-driven.** Every template is a
   Markdown file with a YAML header. The body of the template is *the
   writing space*. Widgets are inserted at named anchor points.
6. **Opt-out, not opt-in.** Default is ON. One toggle in Settings →
   Daily Notes disables everything atomically.
7. **Backwards compatible with Obsidian daily notes.** A Tungsten daily
   journal is just an Obsidian daily note with optional frontmatter.
8. **Privacy by default.** All journal data lives in the user's vault
   and is encrypted at rest if EaaR is enabled.
9. **Pluggable.** Template authors and plugin authors can register new
   widget types via the `registerJournalWidget` API on the native
   extension side.

#### 5.20.2 Feature inventory

| ID | Feature | Description |
| --- | --- | --- |
| J-01 | **Journal home** | Sidebar panel listing entries, calendar heatmap of streak. |
| J-02 | **One-key open today's entry** | `Ctrl/Cmd+Shift+J`. |
| J-03 | **Calendar view of entries** | Month / week / agenda view, color-coded by mood. |
| J-04 | **Streak tracking** | "You've journaled 47 days in a row" badge. |
| J-05 | **Quick-capture from anywhere** | Global hotkey opens popover that appends to today's entry. |
| J-06 | **Template library (curated, 12 built-in)** | Ships with 12 text-driven templates. |
| J-07 | **Template gallery (community)** | Browse and install community templates. |
| J-08 | **Interactive widget rendering** | YAML frontmatter as interactive UI on top of closing `---`. |
| J-09 | **Mood slider** | 1–10 horizontal slider with emoji markers. |
| J-10 | **Energy slider** | 1–10. |
| J-11 | **Anxiety slider** | 1–10. |
| J-12 | **Sleep hours** | Number input with half-hour steps. |
| J-13 | **Sleep quality (1–5 stars)** | Star rating. |
| J-14 | **Gratitude list (3 items)** | Three short text inputs side-by-side. |
| J-15 | **Daily highlight** | One-line text input. |
| J-16 | **Daily low** | One-line text input. |
| J-17 | **What I'd do differently** | Multi-line text area. |
| J-18 | **Habit tracker** | Configurable habits. Each rendered as a checkbox. |
| J-19 | **Photo of the day** | Image attachment picker. |
| J-20 | **Location (optional)** | Free-text or geolocation. |
| J-21 | **Weather (auto, opt-in)** | Open-Meteo (default, no key). |
| J-22 | **Word-count goal** | Set a target word count per entry. |
| J-23 | **Prompt of the day** | Rotating journaling prompt. |
| J-24 | **Mood trend chart** | Line chart of mood/energy/anxiety/sleep. |
| J-25 | **Year in Pixels view** | 365-cell grid, each cell colored by mood. |
| J-26 | **Tag journal entries automatically** | `journal`, `journal/YYYY`, `journal/YYYY-MM`. |
| J-27 | **Search within journal only** | Scope Search to `#journal`. |
| J-28 | **Bases-compatible journal view** | `kind: journal` Bases view type. |
| J-29 | **Export journal to PDF** | One-click export of a date range. |
| J-30 | **Encrypted journal entries** | EaaR handles this. |
| J-31 | **Backup & portability** | Plain `.md` files; works in vanilla Obsidian. |
| J-32 | **Opt-out toggle** | Settings → Daily Notes. |
| J-33 | **Daily-note hotkey is the journal hotkey** | No separate hotkey. |

#### 5.20.3 The text-driven widget experience

On the focused line: Markdown source is visible, raw, with no
transformation. On every other line: Markdown renders inline. Widgets
(yournal frontmatter only): rendered as a thin card at the top of the
note, *above* the writing space.

This is what makes the daily journal feel like a **writing tool**, not a
form.

#### 5.20.4 Built-in template library (v1.0 ships 12)

All templates are **plain Markdown files with a YAML header**, and the
body of the template is **text the user writes into**.

The 12 built-in templates:

1. **Minimal** — date, prompt, free write.
2. **Standard** — date, prompt, mood, energy, gratitude (3), free write.
3. **Reflective** — date, prompt, mood, energy, anxiety, highlight, low, free write.
4. **Stoic morning** — date, prompt, mood, energy, focus, intention, free write.
5. **Stoic evening** — date, prompt, mood, energy, highlight, low, what I'd do differently, free write.
6. **Bullet Journal** — date, prompt, tasks, habits, mood, gratitude, notes.
7. **Mood Tracker** — date, mood, energy, anxiety, sleep, free write.
8. **Therapy Companion** — date, prompt, mood, anxiety, highlight, low, free write.
9. **ADHD Daily** — date, prompt, mood, energy, focus, top 3, habits, free write.
10. **Fitness Log** — date, sleep, exercise, meals, mood, free write.
11. **Reading Journal** — date, book, pages, highlight, low, free write.
12. **Dream Journal** — date, sleep, dream, mood, free write.

All 12 are editable, forkable, and installable from the community gallery.

---

## 6. License

Dual-licensed under **GPL-3.0** OR **Apache-2.0** (at the user's option),
inherited from `zed-industries/zed`. All new code we contribute is under
the same dual-license.

Native extensions written against the Tungsten Native Extension API may
use any license. Plugins written against the Obsidian compat API inherit
the original plugin author's license terms (typically MIT or Apache-2.0).

---

## 7. Open questions

1. **JS engine for the Obsidian compat subsystem.** V8 via `deno_core` (heavier,
   more accurate) vs. QuickJS (lighter, less accurate). Likely: V8 — the
   perf hit at startup is acceptable since the subsystem only runs when
   Obsidian compat is enabled.
2. **DOM polyfill.** `happy-dom` (faster, smaller, less complete) vs.
   `jsdom` (slower, larger, more complete) vs. a custom subset optimized for
   Obsidian plugins. Likely: `happy-dom` for v1, custom subset for v2.
3. **Should the Daily Journal live in the compat subsystem, in case
   Obsidian ships a "Daily Journal" core plugin?** No — Daily Journal is a
   Tungsten differentiator, not Obsidian parity. It stays on the native
   side.
5. **Should the compat subsystem be process-isolated or thread-isolated?**
   Process. A crash in a third-party Obsidian plugin must not take down
   Tungsten. The IPC overhead is acceptable because most plugin calls are
   coarse-grained (file read, editor command).
6. **Should mobile use the same compat subsystem?** Yes, but only the JS
   engine. The DOM polyfill is unnecessary on mobile (no embedded
   webviews). Decision deferred to M8.

---

*— Tungsten core team, 2026-07-12*
