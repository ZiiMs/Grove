# Grove

One desktop home for every AI coding model — with the project's intent, not just its chat, kept in
view.

Grove talks to Anthropic, OpenAI, Google, OpenRouter and OpenCode Go directly: pick a provider, a
model and an API key, and Grove runs the agent itself. Around that loop sits the layer each vendor leaves out: a skill
catalog, a spec graph, a TODO plan that survives restarts, a git/PR panel, and an MCP server viewer.

- Intent of record: `goal-and-requirements.md`
- How it is built: `architecture.md`, with changed decisions in `docs/adr/`
- Terms used by the code: `GLOSSARY.md`
- Why providers replaced harnesses: `docs/adr/0003-providers-replace-harnesses.md`

## Status

First build, feature-complete and hardened. All fifteen tickets in `.scratch/grove/issues/` are
`done`; the dependency graph is in `.scratch/grove/spec.md`. The redesigned shell from
`design-reference/` is now the production UI: a project tree, an always-mounted conversation, a Git ·
Plan · Spec context column, and a settings sheet with seven themes. The decision record is
`docs/adr/0002-workspace-projects-and-the-redesigned-shell.md`. The gates below are run locally —
there is no CI — and the manual smoke checklist is the part no test can paint for you.

## Prerequisites

- Windows 10/11 (the supported build; macOS/Linux are unverified)
- Node.js 22 and npm 10+
- Rust stable with the MSVC target (`rustup default stable-msvc`)
- WebView2 runtime (preinstalled on Windows 11)
- `git` on `PATH` (the Git panel shells out to it); `gh` is optional and only needed to open a draft PR
- A provider API key (Anthropic, OpenAI, Google, OpenRouter or OpenCode Go). Settings → Providers
  stores it in the OS keychain; the vendor's own environment variable works as a fallback.
- [sccache](https://github.com/mozilla/sccache) and
  [cargo-nextest](https://github.com/nextest-rs/nextest) on `PATH` for the configured Rust build and
  test loop (`cargo install --locked sccache cargo-nextest`).

## Quickstart: your first session

```sh
npm ci               # install the workspaces: apps/desktop, packages/contract
npm run tauri dev    # builds the Rust shell and opens the desktop window
```

`npm run dev` alone serves the webview at http://localhost:1420 for UI work; it is not a desktop app.

1. **Add a project.** `Add project` in the tree opens the folder picker; a git repository is detected
   and its default branch and remote are filled in. A plain folder is added as a workspace with no
   branches or diffs.
2. **Create a session.** Use the `+` beside `Chats`, a project or a worktree. The dialog picks the
   project (or `Chat - no project`), an optional model (the provider follows from it), and optionally
   creates a worktree on a new branch. Add a provider key in Settings first; the offline provider is
   only registered in preview and test builds.
3. **Send a message.** Type into the composer. The normalized transcript streams deltas, tool calls,
   results and usage as the runtime emits them, and is written to SQLite as it arrives. A turn that
   asks permission appears as a card in the transcript; the tree and titlebar show that the session
   needs you.
4. **Give it project knowledge.** The Spec tab lists the project's spec graph; open one and
   `Attach to session` pins it, and a running session receives the refreshed snapshot immediately. The
   Plan tab shows the session's TODO checklist. A start, resume or provider switch always assembles the
   current pins and plan.
5. **Install a skill.** Settings → Skills lists the cached GitHub catalog; preview a `SKILL.md` and
   install it into the project's `.agents/skills`. With no network, the catalog serves its cache.
6. **See the diff and open a PR.** The Git tab shows the active checkout, per-file counts and
   stage/commit; a file opens the read-only diff. `Commit & open PR` runs `gh pr create` (an
   authenticated `gh` is required); the PR can be opened ready or as a draft.
7. **Restart and resume.** Quit the window and launch it again. Sessions are rows, not processes: the
   open session comes back marked `exited` with its transcript intact. Send the next message and it
   resumes by itself - the runtime receives the resume token it stored, and the transcript keeps both
   turns.

The tree footer shows the session and awaiting counts, and the status bar carries the version from the
core. Only the skill catalog, a session's own model call, and the cached public model catalogue used to
describe reasoning levels touch the network; everything else runs locally.

## Develop

```sh
npm ci            # install the workspaces: apps/desktop, packages/contract
npm run dev       # Vite dev server on http://localhost:1420
npm run tauri dev # the desktop window (runs the dev server for you)
```

## Build and verify

```sh
npm run typecheck
npm test
npm run build        # Vite production build + the bundle guard
npm run check:bundle # the bundle guard alone, against an existing dist

cargo nextest run -p grove-core
cargo test -p grove-core --doc # nextest does not run doctests
cargo check -p grove-desktop
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

### Fast Rust builds

Rust builds go through [sccache](https://github.com/mozilla/sccache) and tests through
[nextest](https://github.com/nextest-rs/nextest), both configured in `.cargo/config.toml` and
`Cargo.toml`. Install them once:

```sh
cargo install --locked sccache cargo-nextest
```

or download the Windows x86_64 releases from those repositories. `sccache --show-stats` reports the
cache hit rate.

sccache cannot cache incrementally compiled crates and refuses to run when `CARGO_INCREMENTAL=1`, so
the profiles keep incremental off while the wrapper is configured. To go back to the incremental
local loop, remove `rustc-wrapper` and the `CARGO_INCREMENTAL` pin from `.cargo/config.toml` and set
`incremental = true` in the dev and test profiles.

For the edit loop without changing that setup, run `npm run check:rust` (or
`npm run check:rust -- -p grove-desktop`). It typechecks incrementally with the wrapper disabled;
the full gates above still use sccache.

`npm run build` ends with `scripts/check-bundle.mjs`: it fails if Monaco reappears in the entry
chunk or the entry chunk exceeds its size budget.
There is no CI. Run these gates yourself before landing a change, in this order: install,
typecheck, vitest, build (with the bundle guard), rustfmt `--check`, clippy with `-D warnings`,
`cargo nextest run -p grove-core`, and `cargo check -p grove-desktop`.

The cross-cutting Rust suites that pin the release promise:

| Suite | What it proves |
|---|---|
| `tests/session_runtime.rs` | one turn against the offline provider: deltas, message, tool call, usage, idle — persisted in the order the UI saw it |
| `tests/session_restart.rs` | create → send → close → reopen the store → resume; transcript intact, resume token reused |
| `providers::agent` tests | a provider with no key fails before any request; an unanswered approval is refused, not parked |
| `providers::anthropic/openai/google` tests | each vendor's stream decoder against recorded frames, including unknown-frame tolerance |

## Manual smoke checklist

Automated tests cover the core and the webview in isolation; the window itself needs a human:

- [ ] `npm run tauri dev` opens a window with no native frame, a custom titlebar (Grove, git chip,
      awaiting chip and minimize/maximize/close), the project tree, the centre conversation, the
      context column and the status bar. Dragging the titlebar moves the window.
- [ ] `Add project` detects a git folder's branches/remote and adds a plain folder without git.
- [ ] A mock session can be created (with or without a new worktree), given a message, and streams a
      reply; the tree shows it under its project or `Chats`.
- [ ] A permission request shows as a transcript card, marks the session "awaiting you", and
      approving/denying resolves it.
- [ ] The Git tab stages a file, commits, opens the diff, and opens a PR (ready or draft) through an
      authenticated `gh`; the titlebar chip shows the branch and real +/− counts.
- [ ] The Plan tab shows the session checklist, and the Spec tab pins a spec; the composer shows the
      pinned chip.
- [ ] Settings switches all seven themes in dark and light, installs/removes a skill, adds or removes a
      provider key, and probes an MCP server.
- [ ] Quitting and relaunching leaves the session `exited` with its transcript; sending a message
      resumes it and the second turn appears after the first.
- [ ] At 390px wide the window shows Navigation · Session · Context tabs with no horizontal scroll.

## Performance notes

The ~200 MB idle-memory goal is a design constraint, not an afterthought:

- **The event list is virtualized.** `@tanstack/react-virtual` mounts only the transcript rows in view,
  so a 10 000-event session costs the same as a short one.
- **The webview has no code editor.** Spec and plan surfaces are read-only, so Monaco and
  `markdown-it` are not dependencies and never parse at launch.
- **Theme fonts are local.** The seven theme families ship as latin WOFF2 subsets in
  `apps/desktop/src/styles/fonts/`, so a launch has no font request and every theme renders offline.
- **The bundle guards it.** `scripts/check-bundle.mjs` runs after `npm run build` and fails if
  `monaco-editor` appears in the entry chunk or the entry exceeds 600 kB, then reports the largest
  lazy chunks. The successful output ends with a line like
  `Bundle guard: entry index-…js 299.7 kB (budget 600 kB); lazy …`.
- **No background polling on the session path.** Plans are reconciled by a read-time hash check and
  re-read when the Plan tab opens; sessions do not poll the network. The only background fetches are
  the skill catalog, the provider model list after boot or a key change, and the cached
  model-capability catalogue; each falls back to its cache or static list.

## Layout

| Path | What it is |
|---|---|
| `crates/grove-core` | Durable core: providers, the agent runtime, sessions, store, spec graph, plan, skills, MCP, git. No Tauri. |
| `apps/desktop/src-tauri` | Thin Tauri shell: window, commands, event fan-out. |
| `apps/desktop/src` | React webview: shell, chat, context column, settings, dialogs, stores, typed IPC bridge. |
| `packages/contract` | zod schemas + TS types for the IPC contract, validated against `contract/fixtures/`. |
| `contract/fixtures` | Golden JSON written by Rust tests and parsed by TS tests — the drift alarm. |
| `apps/desktop/tools` | Screenshot harness for visual comparison (not part of `npm test`). |
| `scripts/check-bundle.mjs` | Post-build performance guard (entry budget; no editor at launch). |

## Local state

Grove keeps projects, sessions, the event log, plans, pins and settings in
`%APPDATA%/Grove/grove.db`. Project state stays where it belongs: specs are repo files, worktrees are
git, skills live in `~/.agents/skills` or the project's `.agents/skills`, and the git repo is git.
`.grove/` in a project holds per-session plan mirrors and is gitignored.

Provider API keys live in the OS keychain, never in the database or in `settings`.
