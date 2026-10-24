# Grove

One desktop home for every AI coding harness — with the project's intent, not just its chat, kept in
view.

Grove runs Claude Code, Codex, generic ACP agents, OpenCode and `pi` behind one interface, and wraps them
in the layer each harness reinvents badly: a skill catalog that installs into the harness you use, a spec
graph, a TODO plan that survives restarts, a git/PR panel, and an MCP server viewer.

Read `goal-and-requirements.md` for intent and `architecture.md` for how it is built.

## Status

First build, in progress. Work is tracked as tickets in `.scratch/grove/issues/`; the dependency graph
is in `.scratch/grove/spec.md`.

## Prerequisites

- Windows 10/11 (the supported build; macOS/Linux are unverified)
- Node.js 22 and npm 10+
- Rust stable with the MSVC target (`rustup default stable-msvc`)
- WebView2 runtime (preinstalled on Windows 11)

## Develop

```sh
npm ci            # install the workspaces: apps/desktop, packages/contract, sidecar
npm run dev       # Vite dev server on http://localhost:1420
npm run tauri dev # the desktop window (runs the dev server for you)
```

## Verify

```sh
npm run typecheck
npm test
npm run build

cargo test -p grove-core
cargo check -p grove-desktop
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

CI runs exactly these gates on `windows-latest` (`.github/workflows/ci.yml`).

## Layout

| Path | What it is |
|---|---|
| `crates/grove-core` | Durable core: adapters, sessions, store, spec graph, plan, skills, MCP, git. No Tauri. |
| `apps/desktop/src-tauri` | Thin Tauri shell: window, commands, event fan-out. |
| `apps/desktop/src` | React webview: panels, stores, typed IPC bridge. |
| `packages/contract` | zod schemas + TS types for the IPC contract, validated against `contract/fixtures/`. |
| `sidecar` | Node process for JS-only harness SDKs (`pi`), speaking Grove Sidecar Protocol v1. |
| `contract/fixtures` | Golden JSON written by Rust tests and parsed by TS tests — the drift alarm. |

## Manual smoke checklist

Automated tests cover the core and the webview in isolation; the window itself needs a human:

- [ ] `npm run tauri dev` opens a window titled Grove with a seven-panel sidebar.
- [ ] The sidebar footer shows the version and platform from the core.
- [ ] Switching panels mounts only the selected panel.
- [ ] A session can be created, given a message, and survives an app restart (once ticket 02 lands).
- [ ] A skill previews and installs into a chosen directory (ticket 05).
- [ ] A diff renders and a draft PR opens (ticket 04, requires `gh`).

## Local state

Grove keeps sessions, the event log, plans, pins and settings in `%APPDATA%/Grove/grove.db`. Project
state stays where it belongs: specs are repo files, skills live in each harness's own directory, and the
git repo is git. `.grove/` in a project holds per-session plan mirrors and is gitignored.

Tokens live in the OS keychain, never in the database or in `settings`.
