# Tungsten — Roadmap

**Status:** Active development (v0.3 — Zed-based)
**Last updated:** 2026-07-15
**Horizon:** 2026 Q3 → 2029
**Total duration:** ~6 months to v1.0 alpha, 12–18 months to v1.0 stable,
then post-1.0 features through 2029.
**Substrate:** Rust + GPUI (fork of `zed-industries/zed`).

## Quick status — what's done vs. what's left

| Phase | Status | Key deliverables |
| --- | --- | --- |
| **M0.1** Zed fork + rebrand | ✅ **Done** | Fork zed, rebrand to Tungsten, first native build, CI green |
| **M0.2** Tungsten shell MVP | 🟢 **Data layer done** | `tungsten_workspace` crate with 295 tests, vault detection, `.obsidian/` loader, `TungstenVaultStatusItem` in status bar, GPUI `TungstenVaultName` global, daily-note action |
| **M1.1** Markdown on Zed | ✅ **Done (substrate)** | Zed already ships tree-sitter-markdown + Live Preview; rebrand to Tungsten |
| **M1.2** Knowledge layer (Rust port of Foam) | 🟢 **Data layer done** | `NoteIndex`, link graph (`graph::layout` Fruchterman–Reingold), tags, backlinks, unlinked mentions, daily notes, templates, DQL, search query parser, outline (heading tree) |
| **M2.1** Sidebar features | 🟢 **Data layer done** | `panel` module: file tree, tags, backlinks, properties, bookmarks, outline; graph layout + `tungsten-graph` CLI; quick switcher |
| **M2.2** Daily Journal | 🟢 **Data layer done** | `journal` module: 12 default widgets (mood, weather, gratitude, goals, notes, ideas, reading, meals, exercise, sleep, tomorrow, reflection), `calendar()`, `mood_trend()`, `journal_home()` |
| **M2.3** Callouts | 🟢 **Parser done** | `Callout` struct + `extract_callouts` parser (note has `callouts: Vec<Callout>`) |
| **M3.x** Obsidian compat subsystem | 🟡 **Foundation done** | `plugin_api` module: `PluginManifest`, `PluginRegistry`, `discover()`, `shim_surface()` (22 hard-coded methods) |
| **M4.x** Canvas, Bases, Theming | 🟢 **Data layer done** | JSON Canvas 1.0 (`canvas`), `.base` evaluator (`bases`), CSS theme parser (`theme`, 28 variables, `base_theme()`, `list_themes()`) |
| **M5.x** EaaR, hardening | 🟢 **Crypto done** | Argon2id (m=19MiB, t=2, p=1) + XChaCha20-Poly1305 (24-byte nonce) in `eaar`; `tungsten-encrypt`/`tungsten-decrypt` CLIs |
| **M6.x** Publish, Web Clipper, Mobile PWA | 🟡 **Publish done** | `publish` module: `render_html`/`render_full_page`/`render_frontmatter_table` with wikilink/embed post-processing; `tungsten-publish` CLI |
| **M7.x** Sync (Etebase, Syncthing) | ❌ **Not started** | E2EE transports, self-hostable server |
| **M8.x** Mobile native + Multiplayer | ❌ **Not started** | iOS/Android native, Yjs collaboration |

## What this looks like today

The `tungsten_workspace` library is the implementation of the data
layer for milestones M0.2 through M6. It compiles to 34 binaries
(`tungsten-*` for each subsystem plus a `twctl` umbrella dispatcher)
and ships with **343 passing unit tests**. It is the foundation the
GPUI view layer and the eventual Obsidian compat runtime will sit
on top of.

### Binaries (run `./target/debug/twctl help` for the list)

- `tungsten` — main GUI (forked from `zed`, rebrand only)
- `twctl` — umbrella command dispatching to the per-task tools
- `tungsten-vault`, `tungsten-init`, `tungsten-demo` — vault
  detection, sidecar state, example-vault scaffolder
- `tungsten-index`, `tungsten-query`, `tungsten-shell`,
  `tungsten-rename`, `tungsten-backlinks`, `tungsten-grep`,
  `tungsten-find-broken` — knowledge layer (M1.2)
- `tungsten-switcher`, `tungsten-outline`, `tungsten-graph`,
  `tungsten-graph-viz`, `tungsten-graph-stats` — sidebar
  data and graph views (M1.2 / M2.1)
- `tungsten-canvas`, `tungsten-canvas-list`, `tungsten-publish` —
  Canvas / publish (M4 / M6)
- `tungsten-encrypt`, `tungsten-decrypt` — EaaR (M5)
- `tungsten-plugins`, `tungsten-themes`, `tungsten-templates` —
  Obsidian compat foundation (M3)
- `tungsten-smart`, `tungsten-yip`, `tungsten-journal` —
  M2.2 surface tools
- `tungsten-inspect`, `tungsten-validate`, `tungsten-doctor`,
  `tungsten-stats`, `tungsten-diff`, `tungsten-tasks`,
  `tungsten-mood`, `tungsten-random`, `tungsten-export`,
  `tungsten-sync`, `tungsten-snippet` — diagnostic and
  maintenance CLIs

---

## Reading guide

- **Milestones (M)** are time-boxed chunks of 1–6 weeks. We move fast.
- **Phases** group milestones into coherent deliverables.
- **Gates** are exit criteria — without them, a milestone isn't done.
- **Every milestone has a working binary** users can install and try.
- We ship in the open. Every public release has a changelog and a public demo vault.
- The **Obsidian compat subsystem** is a separate, isolated subsystem (§4.6 in PRD).
  Breaking changes to the native extension API are P0. Breaking the
  compat subsystem is acceptable per its compatibility target (top 20
  Obsidian plugins by v1.0).

---

## Business model reminder

- **Core app:** FOSS, GPL-3.0 + Apache-2.0 dual-licensed, free forever,
  no telemetry.
- **Sync:** paid subscription (Etebase-hosted) or **self-hostable for free**
  (Etebase-server, Syncthing, or any Etebase-protocol server like Joplin's).
- **Publish:** paid subscription for hosted, or **self-hostable for free**
  (static output to any host).
- **No vendor lock-in.** A user can take their vault and leave at any time.

---

## Phase 0 — Fork Zed & First Build (Q3 2026, ~4 weeks)

**Goal:** stand up the Tungsten repository on top of Zed, get a working
native build, rebrand the shell. Prove the substrate works.

### M0.1 — Zed fork, rebrand, first build (Weeks 1–2)

- [x] Fork `github.com/zed-industries/zed` as `github.com/fuegocoding/tungsten`.
- [x] Full clone (38,958 commits) preserved in git history.
- [x] Update top-level docs: `PRD.md`, `ROADMAP.md`, `README.md`.
- [x] Update license notice: GPL-3.0 + Apache-2.0 (inherited).
- [ ] Set up CI: GitHub Actions runs `./script/build` on Linux, macOS, Windows.
- [ ] Add Contributor Covenant 2.1 + DCO.
- [ ] Rename product strings: "Tungsten" replaces "Zed" in window title,
      app menu, about dialog, settings UI, command palette.
- [ ] Replace logos and brand assets (icons, splash, favicon).
- [ ] Strip any Zed-specific telemetry / cloud / auth code (already mostly
      optional in Zed — verify).
- [ ] First native build: `tungsten-alpha-0.0.1` — basically Zed with
      Tungsten branding.
- [ ] Reorganize the workspace: introduce a `tungsten/` virtual workspace
      in `Cargo.toml` so the Tungsten crates live alongside Zed's
      unmodified crates. Zed stays upstream-trackable; Tungsten additions
      live in their own crates.

**Gate:** `tungsten-alpha-0.0.1` runs on all 3 OSes; CI green; the app
opens a folder of `.md` files in Zed's default editor.

### M0.2 — Tungsten shell MVP (Weeks 3–4)

> **Note:** this milestone was a 12-week push in v0.2. In v0.3 it's
> dramatically shorter because Zed's shell already exists — we just need
> to add the vault concept and hide code-editor affordances.

- [ ] `tungsten_workspace` crate: vault concept, multi-vault, vault
      switcher ribbon icon, "Open folder as vault" / "Create new vault"
      / "Open from Obsidian folder" commands.
- [ ] `.obsidian/` config dir loader (themes, plugins, hotkeys, layouts,
      snippets) — read-only at first, write-back after M0.3.
- [ ] Hide by default (Settings → Workspace): terminal, debugger, Git
      panel, tests, AI assistant, code completions for `.md`.
- [ ] Restyle the default UI: Obsidian-style spacing, font, accent color.
- [ ] Add a top-level "Vault" settings tab.
- [ ] Add the left ribbon (Obsidian-style, 8 icons by default).
- [ ] Add the right sidebar (Outline / Backlinks / Properties panels —
      empty at first, populated in M2.1).
- [ ] Status bar: backlinks count (placeholder), mode, word count, sync
      status (placeholder), current vault name.
- [ ] Command palette: pin recent Tungsten commands.
- [ ] Quick switcher: vault-wide, fuzzy.
- [ ] Welcome tab on first run (sample notes, links to docs, "Open
      existing Obsidian vault" wizard).

**Gate:** opening an existing Obsidian vault loads without errors; themes
and hotkeys from `.obsidian/` are respected; the ribbon and sidebars are
visible; a typical Obsidian user can switch to Tungsten and feel at home
in under 5 minutes.

**Deliverable:** `tungsten-alpha-0.0.3` — the first public binary that
looks and feels like a note-taking app, even though the editor is still
Zed's default.

---

## Phase 1 — Markdown & Knowledge Layer (Q4 2026, ~12 weeks)

**Goal:** turn Zed into a Markdown editor with Live Preview, and bring up
the knowledge layer (Foam-equivalent, in Rust).

### M1.1 — Markdown on Zed (Weeks 5–8)

- [ ] Add `tree-sitter-markdown` and `tree-sitter-markdown-inline` to
      Zed's language registry.
- [ ] Add Markdown file extension support to the buffer (already
      supported; verify the markdown language config is right).
- [ ] Disable code completions and LSP diagnostics for `.md` files by
      default (Settings → Workspace).
- [ ] `tungsten_markdown::decorate(buffer, tree) -> Vec<Decoration>`:
      produces GPUI decorations for inline formatting (bold, italic,
      strikethrough, highlight, inline code, headings).
- [ ] Source mode: zero decorations.
- [ ] Reading mode: full render, no editing.
- [ ] Live Preview mode: focus-line rule (the Obsidian behavior).
- [ ] Wikilink decorations: `[[Name]]` rendered as link text.
- [ ] Wikilink autocompletion on `[[` (fuzzy, with alias, with
      `##header` and `#^id`).
- [ ] Tag pills (`#tag`).
- [ ] Code block syntax highlighting (using tree-sitter grammars).
- [ ] Mermaid inline preview (rendered in a webview pane).
- [ ] MathJax inline + block (using KaTeX in a webview pane).
- [ ] Tables rendered with column alignment.
- [ ] Task list checkboxes clickable.
- [ ] Mode toggle: `Ctrl+E` (Source / Live Preview / Reading).
- [ ] Soft line breaks (Shift+Enter = `<br>`), strict line breaks (toggle).

**Gate:** Live Preview renders all the example vaults in `obsidian-help`
correctly; the focus-line rule is exactly Obsidian's.

### M1.2 — Knowledge layer (Rust port of Foam) (Weeks 9–14)

- [ ] `tungsten_workspace` knowledge submodule: Markdown parsing
      (links, tags, frontmatter, blocks, sections, callouts).
- [ ] `TungstenWorkspace` indexed in memory, persisted to
      `vault/.tungsten/index.db` (SQLite via `rusqlite`).
- [ ] `TungstenGraph`: link graph (nodes = notes, edges = wikilinks).
      Force-directed layout using GPUI's GPU canvas.
- [ ] `FoamWorkspace`-style features: link integrity (rename with wikilink
      rewriting), note creation, templates, daily note, unique note.
- [ ] Tags index, backlinks index, orphans detection, unlinked mentions
      detection.
- [ ] DQL (Dataview Query Language) parser + executor.
- [ ] JS expression language (TBD engine — Boa or V8) for embedded
      `js` blocks.
- [ ] Property-based search `[prop]`, `[prop:value]`, `null`.
- [ ] Migration paths: from `.obsidian/` (already in M0.2), from
      `.foam/` (Foam workspace), from Day One, from CSV.

**Gate:** the `obsidian-help` sample vault opens, indexes, and
correctly resolves every wikilink; the `foam-sample` workspace
opens, indexes, and links integrity-preserves renames.

**Deliverable:** `tungsten-beta-0.1.0` — usable as a Foam replacement on
top of Zed, with Live Preview working.

---

## Phase 2 — Sidebar Features & Daily Journal (Q1 2027, ~12 weeks)

**Goal:** ship the sidebar panels (the day-to-day UX) and the Daily
Journal differentiator.

### M2.1 — Sidebar features (Weeks 15–18)

- [ ] **Files** panel: Zed's project panel, restyled as Obsidian's file
      explorer (tree, drag-drop, sort, auto-reveal, multi-select).
- [ ] **Search** panel: Obsidian operators (`file:`, `tag:`, `[prop:]`,
      regex, etc.). Sort, copy results, explain search.
- [ ] **Bookmarks** panel: files, folders, graphs, searches, headings,
      blocks, links. Reorderable groups.
- [ ] **Tags** panel: hierarchical, click-to-search, Ctrl/Cmd-click to
      add to search, sort by name or frequency.
- [ ] **Outline** panel: headings + block IDs. Drag-drop headings in
      outline to reorder in note.
- [ ] **Backlinks** panel: incoming links + unlinked mentions.
- [ ] **Outgoing links** panel: outgoing + unlinked mentions.
- [ ] **Properties** panel: file & vault-wide.
- [ ] **Graph** panel: local graph (per-note).
- [ ] Page preview: hover popover with rendered note.
- [ ] Footnotes view.
- [ ] Multi-select with Alt/Shift-click in Files.

**Gate:** a typical Obsidian user can move through their vault using only
the sidebars and the quick switcher, without touching the file explorer.

### M2.2 — Daily Journal (Weeks 19–24) — *Tungsten differentiator*

> **Note:** this is the same scope as v0.2's M2.4, but on the native side
> of the architecture. The widget registry is a native extension API
> (`registerJournalWidget`); the subsystem is a single native extension
> (`tungsten-journal`).

- [ ] `tungsten_journal` crate + `tungsten-journal` native extension.
- [ ] `registerJournalWidget` API: native extensions register custom
      widget types (slider, scale, multi-select, number, text-area, photo,
      location, time-range, mood-board, habit-tracker, etc.).
- [ ] `journalWidgetExtension`: buffer decoration that renders the YAML
      frontmatter as an interactive widget card on top of the closing
      `---`.
- [ ] Built-in widgets (all of these, see PRD §5.20.2): mood slider,
      energy slider, anxiety slider, sleep hours, sleep quality stars,
      gratitude list (3), highlight, low, habit tracker, photo, location,
      weather (Open-Meteo), word-count goal, prompt of the day.
- [ ] `JournalRenderer` — maps YAML keys to widgets, atomic `vault.process`
      on change.
- [ ] **Journal home panel** (sidebar): list of entries, calendar
      heatmap, streak counter, mood trend chart (last 30/90/365 days),
      Year-in-Pixels view (365 cells, click to open).
- [ ] **`Ctrl/Cmd+Shift+J`** hotkey for "open today's entry."
- [ ] Global quick-capture popover (configurable hotkey).
- [ ] **12 built-in templates** (Minimal, Standard, Reflective, Stoic
      morning, Stoic evening, Bullet Journal, Mood Tracker, Therapy
      Companion, ADHD Daily, Fitness Log, Reading Journal, Dream Journal).
- [ ] **Template gallery** — `community-journal-templates.json`.
- [ ] Weather widget: Open-Meteo (no API key), opt-in geolocation.
- [ ] Auto-tagging (`journal`, `journal/YYYY`, `journal/YYYY-MM`).
- [ ] `kind: journal` Bases view type.
- [ ] Export date range to PDF.
- [ ] Migration paths: from Obsidian daily notes, Day One, CSV.
- [ ] Accessibility: full keyboard nav, ARIA roles, high-contrast theme.
- [ ] Performance: 5-year journal (1,825 entries) loads in < 500 ms;
      Year-in-Pixels renders in < 16 ms (one frame).
- [ ] Documentation: user guide, template author guide, plugin author
      guide (`registerJournalWidget`).

**Gate:** the 12 built-in templates work end-to-end; the Journal home
panel renders a 5-year fake dataset in < 500 ms; the
`registerJournalWidget` API accepts and renders a sample community
plugin's custom widget; all widgets are keyboard-accessible.

**Deliverable:** `tungsten-beta-0.2.1` — Daily Journal feature complete.
Tungsten is now a credible Day One / Stoic. / Reflectly alternative, in
addition to the Obsidian parity.

---

## Phase 3 — Obsidian Compat Subsystem (Q2 2027, ~14 weeks)

**Goal:** ship the isolated subsystem that runs unmodified Obsidian
plugins. The biggest architectural risk in the project.

> **Why isolated.** Per PRD §4.6: the compat subsystem is a separate
> process, has its own JS runtime, its own DOM polyfill, and talks to
> Tungsten core over IPC. It is independent of the native extension
> system. If the user disables it, it is not launched at all — zero
> runtime cost.

### M3.1 — Compat subsystem shell (Weeks 25–28)

- [ ] `tungsten_obsidian_compat` crate (in the Tungsten main process):
      process launcher, IPC protocol, lifetime management.
- [ ] Choose JS engine: V8 via `deno_core` (default) or QuickJS.
- [ ] Choose DOM polyfill: `happy-dom` (default) or custom subset.
- [ ] `tungsten_obsidian_compat_runtime` crate: the standalone process
      binary. Embeds the JS engine + DOM polyfill.
- [ ] IPC protocol over Unix socket / Windows named pipe / macOS XPC:
      `vault.read`, `vault.modify`, `vault.list`, `workspace.getActiveFile`,
      `editor.replaceRange`, `markdownView.show`, `notice.show`, etc.
      This is the "Obsidian app surface" as an RPC.
- [ ] `obsidian` module shim: TypeScript types vendored from
      `obsidian-developer-docs`, exported into the JS runtime.
- [ ] `App`, `Plugin`, `Workspace`, `Vault`, `MarkdownView`, `Editor`,
      `MarkdownPostProcessor`, `Component` re-implemented in TS/Rust and
      bridged to Tungsten core over IPC.
- [ ] Manifest adapter: an Obsidian plugin's `manifest.json` is loaded
      as a Tungsten-compatible manifest.
- [ ] Plugin store: read `obsidian-releases/community-plugins.json`,
      install/uninstall, restricted mode, hot-reload.

**Gate:** the process boundary is solid; the IPC is round-tripped; the
runtime launches with a 200 ms cold-start penalty and a 50 MB RSS cost.

### M3.2 — Compatibility sweep (Weeks 29–34)

- [ ] `obsidian` module: `Modal`, `Setting`, `Notice`, `EditorSuggest`,
      `FuzzySuggestModal`, `PluginSettingTab`, `MarkdownRenderer`,
      `MarkdownPostProcessor`, `Editor`, `Workspace`, `Vault`, `FileManager`.
- [ ] All `registerX` methods (`registerView`, `registerEditorExtension`,
      `registerMarkdownPostProcessor`, `registerEvent`, etc.).
- [ ] All `obsidian` events (`vault.on('create')`, `workspace.on('file-open')`,
      etc.) bridged over IPC.
- [ ] Restricted mode: the runtime can be sandboxed further (no IPC, no
      network, no filesystem).
- [ ] Plugin safety scanning: static analysis + manifest checks.
- [ ] Compatibility CI: nightly run against top 100 plugins. Fix gaps.
      Prioritize:
      1. Dataview, Tasks, Templater, Kanban, Calendar (top 5 by install
         count).
      2. Various Complements, Quickadd, Outliner, Natural Language Dates,
         Linter.
      3. Excalidraw (→ Canvas), Dataview Serializer, Mind Map, Spaced
         Repetition.
      4. Remotely Save, Self-hosted LiveSync.
- [ ] Community outreach to plugin authors.
- [ ] Documentation: writing an Obsidian plugin that runs in Tungsten
      (mostly identical, with a few IPC-perf caveats).

**Gate:** ≥ 80% of the top 100 plugins install and run. Top 20 work
flawlessly. The remaining 20% have filed compatibility issues with
workarounds.

**Deliverable:** `tungsten-beta-0.3.0` — first version most existing
Obsidian users can adopt. Their existing plugins just work.

---

## Phase 4 — Canvas, Bases, Theming (Q2–Q3 2027, ~12 weeks)

**Goal:** ship the three big "v1 of Obsidian"-defining modules that
aren't editor or shell.

### M4.1 — Canvas (Weeks 35–38)

- [ ] `tungsten_canvas` crate + native extension: `.canvas` file format
      reader/writer (JSON Canvas spec).
- [ ] Infinite 2D canvas on GPUI (PanZoom, Scroll, GPU-rasterized
      cards).
- [ ] Cards: text, note, media, web page, folder.
- [ ] Connections: directed, labeled, colorable.
- [ ] Groups: empty or from selection.
- [ ] Embed in note (`![[my.canvas]]`).
- [ ] Search-within-canvas.
- [ ] Drag-drop attachments and folders.
- [ ] Right-click connection → Go to source/target.

**Gate:** every `.canvas` example in the JSON Canvas spec works
identically.

### M4.2 — Bases (Weeks 39–42)

- [ ] `tungsten_bases` crate + native extension: `.base` file format
      reader/writer.
- [ ] View types: Table, List, Cards, Map (built-in).
- [ ] Filter / sort / group by properties.
- [ ] Formulas with Functions library.
- [ ] Summaries.
- [ ] Embed via `base` code block in note.
- [ ] `registerBasesView` for plugins.
- [ ] Implement Obsidian's Map and Kanban view types as built-ins.

**Gate:** every `.base` example from Obsidian's help renders the same in
Tungsten.

### M4.3 — Theming & polish (Weeks 43–46)

- [ ] All Obsidian CSS variables (light + dark) mapped to GPUI theme
      tokens.
- [ ] Community themes browser (download from
      `community-css-themes.json`).
- [ ] CSS snippets in `.obsidian/snippets/`.
- [ ] `cssclasses` property.
- [ ] Custom callouts via CSS.
- [ ] Light/dark/auto modes.
- [ ] Theme update checker.
- [ ] Style Inspector.
- [ ] Accessibility audit: keyboard nav, screen reader, high contrast.
- [ ] Internationalization: 11 languages (en, ar, de, es, fr, it, ja, ko,
      pt-BR, ru, zh-CN).
- [ ] Right-to-left (RTL) UI.

**Gate:** all top 50 community themes install and render correctly.
RTL works. 11 languages translated.

**Deliverable:** `tungsten-rc-1.0.0-rc1` — feature-complete. Bug-bash
begins.

---

## Phase 5 — Encryption at Rest + Hardening (Q3 2027, ~10 weeks)

**Goal:** ship the encryption layer, harden for v1.0, write the docs.

### M5.1 — Encryption at Rest (Weeks 47–50)

- [ ] `tungsten_eaar` crate + native extension.
- [ ] `age` (preferred) + `libsodium` (fallback) integration via Rust
      crates.
- [ ] XChaCha20-Poly1305 AEAD.
- [ ] Argon2id KDF.
- [ ] `TungstenDataAdapter` for transparent mount: a vault stored as
      `vault.age` appears as a normal folder when unlocked.
- [ ] Per-vault passphrase; key file support (v2: YubiKey).
- [ ] Encryption onboarding wizard.
- [ ] "Change passphrase" command.
- [ ] Performance: < 5% overhead on 10k-note vault.
- [ ] FUSE on macOS/Linux, Dokany on Windows.
- [ ] Third-party crypto audit (pre-launch).

**Gate:** an encrypted vault is byte-for-byte indistinguishable from a
manually-encrypted one; can be decrypted with a CLI tool using the
passphrase.

### M5.2 — Hardening (Weeks 51–56)

- [ ] SBOM published per release (CycloneDX).
- [ ] Security policy + vulnerability disclosure.
- [ ] Code signing: macOS Developer ID, Windows EV cert.
- [ ] Auto-update channel.
- [ ] Crash reporter: local-only by default, opt-in community Sentry.
- [ ] Performance pass: cold-start < 500 ms, idle memory < 200 MB.
- [ ] Telemetry-free verification (audit: zero outbound calls in offline
      mode).
- [ ] Full plugin compatibility CI on every commit (native + compat).
- [ ] Test on 10+ Linux distros, 3 macOS versions, 2 Windows versions.
- [ ] Documentation site: user guide, plugin author guide (native +
      compat), theme author guide, admin guide, journal author guide,
      journal plugin author guide.
- [ ] Marketing site, FAQ, comparison page, migration guides (from
      Obsidian, Day One, Daylio, Stoic., Zed, VS Code, Foam).
- [ ] Demo vault downloadable.
- [ ] Demo journal (1 year of fake entries) downloadable.

**Gate:** all open issues from RC are closed; security audit complete;
performance targets met.

**Deliverable:** **`tungsten-1.0.0` — public launch.**

🎉

---

## Phase 6 — Publish, Web Clipper, Mobile PWA (Q4 2027 → Q1 2028, ~16 weeks)

**Goal:** complete the surface area. Ship Publish (free, local-first),
Web Clipper, and the mobile PWA.

### M6.1 — Static Publish (Weeks 57–62)

- [ ] `tungsten_publish` crate + native extension.
- [ ] Static site generator (Rust binary, in-app or CLI).
- [ ] Local output folder (free).
- [ ] Custom domains (Pro/Team sub).
- [ ] Themes for sites.
- [ ] Password protection (Pro/Team sub).
- [ ] Full-text search (Pro/Team sub).
- [ ] SEO meta tags, permalinks, social media link previews.
- [ ] Optional self-hostable "Tungsten Publish" server (Docker, Helm).
- [ ] **Publish a journal feature:** one-click "Publish my journal" with
      privacy-respecting default.

**Gate:** the `obsidian.md/help` site can be generated from the help
vault using Tungsten Publish; a sample journal publishes as a public,
password-protected site.

### M6.2 — Web Clipper (Weeks 63–68)

- [ ] Browser extension (Chrome, Firefox, Safari).
- [ ] Templates with logic (conditionals, loops, fallbacks).
- [ ] Interpreter (NL content extraction).
- [ ] Reader view.
- [ ] Send-to-vault via E2EE channel.

**Gate:** clip a Wikipedia article and a YouTube video and both save
with rich metadata to a Tungsten vault.

### M6.3 — Mobile web (PWA) (Weeks 69–72)

- [ ] Progressive Web App with offline-first storage (IndexedDB +
      Service Worker).
- [ ] Touch-optimized UI (responsive sidebar, gesture nav).
- [ ] Local-only vaults.
- [ ] **Daily Journal on mobile web** — quick-capture, today's entry,
      mood slider, gratitude list, photo upload.
- [ ] Web Clipper support (via extension bridge).
- [ ] "Add to home screen" install.

**Gate:** open a 1,000-note vault on an iPhone via Safari, edit a note,
close the browser, reopen — changes are still there; can complete
today's journal entry (mood + gratitude + free text) in < 60 seconds
from the home screen.

**Deliverable:** `tungsten-1.2.0` — Publish + Web Clipper + mobile web
+ mobile journal.

---

## Phase 7 — Sync (paid sub; self-hostable for free)

**Goal:** ship **two sync transports** — Etebase (E2EE) and Syncthing
(P2P). Both free when self-hosted. Hosted is a paid sub to pay for the
audit and infrastructure.

### M7.1 — Etebase integration (default E2EE)

- [ ] `tungsten_sync` crate + `tungsten-sync-etebase` extension.
- [ ] Embed `etebase` Rust client in Tungsten core.
- [ ] Map: vault ↔ Etebase collection; note ↔ item; attachment ↔ item
      file.
- [ ] **Bring-your-own-Etebase-server**: settings dialog to point at any
      Etebase-protocol server.
- [ ] Multi-device pairing flow.
- [ ] Version history (uses Etebase's built-in revision history).
- [ ] Selective sync (per-folder).
- [ ] Sync log + status icon in Tungsten.
- [ ] Headless CLI: `tungsten sync status`, `tungsten sync pair`,
      `tungsten sync force`.

**Gate:** two Tungsten clients can sync a 1,000-note vault via an
Etebase-protocol server with no plaintext reaching the server. A Joplin
server works.

### M7.2 — Syncthing mode (P2P, no server)

- [ ] `tungsten-sync-syncthing` extension.
- [ ] Auto-detect a Syncthing-managed vault folder.
- [ ] "Sync via Syncthing" toggle in Settings.
- [ ] Document the EaaR + Syncthing path for E2EE.
- [ ] Conflict resolution: per-file LWW + manual merge UI for body
      conflicts (Yjs for CRDT).

**Gate:** a vault is in sync between two devices via Syncthing only —
no Tungsten server touched.

### M7.3 — Hosted sync (paid sub)

- [ ] Multi-region deployment (US, EU, APAC) of `etebase-server` with
      Tungsten-specific defaults.
- [ ] Stripe billing integration.
- [ ] Pricing tiers: **Free** (local-only, EaaR, no hosted sync),
      **Pro $5/mo** (hosted Etebase sync, 1-year history, 10 GB),
      **Team $12/user/mo** (shared vaults, audit log, SSO).
- [ ] 3rd-party crypto audit (cure53 or similar).
- [ ] Public launch.

**Gate:** hosted sync is live, billing works, audit is published.

**Deliverable:** `tungsten-1.2.0` — full Sync product (free + Pro +
Team + self-host).

---

## Phase 8 — Mobile Native + Multiplayer (Q3 2028 → 2029)

**Goal:** the final stretch — native mobile and real-time collaboration.

### M8.1 — iOS + Android native (Q3 2028)

- [ ] Native rewrite (Swift + Kotlin) of the mobile shell. Most of the
      Rust core (buffer, tree-sitter, knowledge layer) compiles to mobile
      via `cargo mobile` or manual `aarch64-linux-android` /
      `aarch64-apple-ios` builds.
- [ ] iOS widgets, Siri/Shortcuts.
- [ ] Android widgets, Quick Tiles.
- [ ] Local-only mode (no sync).
- [ ] **Daily Journal on native mobile** — widgets, photo capture,
      quick-capture, Year-in-Pixels, mood trends.
- [ ] App Store + Play Store launch.

**Gate:** a typical iOS/Android user gets the same experience as on
desktop, including full journal support.

### M8.2 — Real-time multiplayer (2029)

- [ ] Yjs-based collaborative editing.
- [ ] Live cursors, presence.
- [ ] Comments (using Obsidian's comments API as inspiration).
- [ ] Conflict resolution in offline-first scenarios.
- [ ] Audit log of edits.
- [ ] Optional shared journal vaults (read-only or with a partner).

**Gate:** two users can edit the same note in real time with sub-200 ms
latency; two users can co-author a shared journal entry.

**Deliverable:** `tungsten-2.0.0` — Tungsten as a serious FOSS
alternative to Notion + Obsidian + Day One + Google Docs.

---

## Steady-state maintenance (from Q1 2027 onward)

- **Quarterly minor releases** (`.1`, `.2`, `.3`) for bug fixes, plugin
  API extensions, community themes.
- **Monthly patch releases** (`.0.1`, `.0.2`) for security and stability.
- **Nightly plugin compatibility CI** — never break the top 100 plugins
  in either subsystem.
- **Yearly major releases** aligned with Obsidian's release cadence (we
  want to follow new Obsidian features within 6 months).
- **Annual community survey** — feature voting.
- **Annual security audit** for the crypto layer.

---

## Dependency timeline (at a glance)

```
2026 Q3  ▓▓▓▓░░░░ Phase 0: Fork Zed + first build (M0.1–0.2) 🟡
2026 Q4  ████░░░░ Phase 1: Markdown + Knowledge layer (M1.1–1.2)
2027 Q1  ████░░░░ Phase 2: Sidebar features + Daily Journal ⭐
2027 Q2  ██████░░ Phase 3: Obsidian compat subsystem (M3.1–3.2)
2027 Q3  ████░░░░ Phase 4: Canvas, Bases, Theming (M4.1–4.3)
2027 Q4  ████░░░░ Phase 5: Encryption + v1.0 launch 🎉
2028 Q1  ████░░░░ Phase 6: Publish + Web Clipper + Mobile PWA
2028 Q2  ████░░░░ Phase 7: E2EE Sync (Etebase, Syncthing)
2028 Q3  ████░░░░ Phase 8a: Mobile native (with Journal)
2029     ████░░░░ Phase 8b: Real-time multiplayer (Yjs)
```

> **Progress note:** Phase 0 is in progress. The Zed fork is complete;
> rebranding and CI are next. The v0.2 VS Code fork work (50% of M1 done
> in TypeScript) is permanently discarded. v1.0 launch estimate is end
> of Q3 2027, with the v0.2 estimate of 12 months revised to 12–18
> months to account for the new compat subsystem work.

---

## Open questions to revisit

1. JS engine for the Obsidian compat subsystem — V8 (`deno_core`) vs.
   QuickJS.
2. DOM polyfill — `happy-dom` vs. `jsdom` vs. custom subset.
3. Should the compat subsystem be process-isolated (default) or
   thread-isolated (faster IPC, less safety)?
4. Should the daily journal also be available in the compat subsystem
   for plugins that want to register widgets? (Currently no — the
   journal is a native differentiator.)
5. Sync protocol interop with Obsidian Sync — is there user demand?
6. Should we offer a managed Tungsten Cloud (hosted vaults, hosted sync,
   hosted publish) as a single SKU?

---

## How to contribute

- File issues on the [main repo](https://github.com/fuegocoding/tungsten/issues).
- Join the Discord (link TBD).
- Pick a "good first issue" — they are tagged.
- Read `CONTRIBUTING.md` and `CODE_OF_CONDUCT.md`.
- Sign your commits (DCO).
- Be patient, be kind. We're building a cathedral.

— *Tungsten core team, 2026-07-12*
