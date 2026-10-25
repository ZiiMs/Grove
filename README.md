# Grove

One desktop home for every AI coding harness — with the project's intent, not just its chat, kept in
view.

Grove runs Claude Code, Codex, generic ACP agents, OpenCode and `pi` behind one interface, and wraps
them in the layer each harness reinvents badly: a skill catalog that installs into the harness you
use, a spec graph, a TODO plan that survives restarts, a git/PR panel, and an MCP server viewer.

- Intent of record: `goal-and-requirements.md`
- How it is built: `architecture.md`, with changed decisions in `docs/adr/`
- Terms used by the code: `GLOSSARY.md`
- Writing a harness adapter: `docs/adapters.md`; the sidecar boundary: `docs/sidecar-protocol.md`

## Status

First build, feature-complete and hardened. All fifteen tickets in `.scratch/grove/issues/` are
`done`; the dependency graph is in `.scratch/grove/spec.md`. The automated gates below run on
`windows-latest`, and the manual smoke checklist is the part no test can paint for you.

## Prerequisites

- Windows 10/11 (the supported build; macOS/Linux are unverified)
- Node.js 22 and npm 10+
- Rust stable with the MSVC target (`rustup default stable-msvc`)
- WebView2 runtime (preinstalled on Windows 11)
- `git` on `PATH` (the Git panel shells out to it); `gh` is optional and only needed to open a draft PR
- A real harness CLI is optional: the **mock** harness is built in and works with nothing installed.
  `pi` additionally needs the Node sidecar build that `npm run build` writes.

## Quickstart: your first session

```sh
npm ci               # install the workspaces: apps/desktop, packages/contract, sidecar
npm run tauri dev    # builds the Rust shell and opens the desktop window
```

`npm run dev` alone serves the webview at http://localhost:1420 for UI work; it is not a desktop app.

1. **Create a session.** The Sessions panel opens first. Pick a harness — choose `mock` to try Grove
   with no harness installed — optionally set the working directory, and create the session.
2. **Send a message.** Type into the composer. The normalized transcript streams deltas, tool calls,
   results and usage as the harness emits them, and is written to SQLite as it arrives.
3. **Give it project knowledge.** Open a session in Sessions, switch to Specs and pin a spec; the Plan
   panel holds the session's TODO plan. Back in Sessions, `Refresh context` re-reads the pins and plan
   and pushes the snapshot into the running harness; the transcript records what was sent.
4. **Install a skill.** Skills lists the cached GitHub catalog; preview a `SKILL.md` and install it
   into a harness directory (or project-local `.agents/skills`). With no network, the catalog serves
   its cache.
5. **See the diff and open a PR.** Git takes a repository path, shows status and a unified diff, and
   `Open draft PR` runs `gh pr create --draft` (an authenticated `gh` is required).
6. **Restart and resume.** Quit the window and launch it again. Sessions are rows, not processes: the
   open session comes back marked `exited` with its transcript intact. Press `Resume` and continue the
   same conversation — the adapter receives the resume token it stored, and the transcript keeps both
   turns.

The sidebar footer shows the app version and platform from the core. Everything except the GitHub
catalog fetch runs offline; sessions never require the network.

## Develop

```sh
npm ci            # install the workspaces: apps/desktop, packages/contract, sidecar
npm run dev       # Vite dev server on http://localhost:1420
npm run tauri dev # the desktop window (runs the dev server for you)
```

## Build and verify

```sh
npm run typecheck
npm test
npm run build        # Vite production build + sidecar dist + the bundle guard
npm run check:bundle # the bundle guard alone, against an existing dist

cargo test -p grove-core
cargo check -p grove-desktop
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

`npm run build` ends with `scripts/check-bundle.mjs`: it fails if Monaco is inside the entry chunk.
CI runs exactly these gates on `windows-latest` (`.github/workflows/ci.yml`), in this order: install,
typecheck, vitest, build (with the bundle guard), rustfmt `--check`, clippy with `-D warnings`,
`cargo test -p grove-core`, and `cargo check -p grove-desktop`.

The cross-cutting Rust suites that pin the release promise:

| Suite | What it proves |
|---|---|
| `tests/adapters_conformance.rs` | all six adapters (mock + five) pass one conformance suite against their fixtures |
| `tests/adapters_restart.rs` | per adapter: create → send → close → reopen the store → resume; transcript intact, resume token reused |
| `tests/offline.rs` | all six fake harnesses run and the skill catalog serves its cache with `HTTP(S)_PROXY` at a black hole and `NO_PROXY` empty |

## Manual smoke checklist

Automated tests cover the core and the webview in isolation; the window itself needs a human:

- [ ] `npm run tauri dev` opens a window titled Grove with a seven-panel sidebar (Sessions, Skills,
      Specs, Plan, MCP, Git, Settings).
- [ ] The sidebar footer shows the version and platform from the core.
- [ ] Switching panels mounts only the selected panel, and every panel renders real content (no
      placeholder bodies).
- [ ] A mock session can be created, given a message, and streams a reply.
- [ ] A skill previews and installs into a chosen directory, and appears again from cache with the
      network disabled.
- [ ] The Git panel shows status and a diff, and opens a draft PR through an authenticated `gh`.
- [ ] Quitting and relaunching leaves the session `exited` with its transcript; `Resume` continues it
      and the second turn appears after the first.
- [ ] With a real harness installed (Codex, ACP, OpenCode or `pi`), the picker and Settings → Doctor
      report accurate capabilities and detection state; `mock` remains selectable.

## Performance notes

The ~200 MB idle-memory goal is a design constraint, not an afterthought:

- **Panels mount on demand.** `apps/desktop/src/App.tsx` renders only the active panel; every other
  panel's component is unmounted, and its state lives in the zustand store, not the DOM.
- **The event list is virtualized.** `@tanstack/react-virtual` mounts only the transcript rows in view,
  so a 10 000-event session costs the same as a short one.
- **Monaco is lazy and only for editing.** The diff viewer and spec editor reach it through
  `React.lazy`/`import()`, so a window launch never parses the editor; it loads on first open and is
  never mounted for chat markdown (`markdown-it`, HTML disabled).
- **The bundle guards it.** `scripts/check-bundle.mjs` runs after `npm run build` and fails if
  `monaco-editor` appears in the entry chunk or if no lazy chunk contains it. The successful build
  output ends with a line like
  `Bundle guard: entry index-…js 390.7 kB (no Monaco); lazy index-…js 14.2 kB`.
- **No background polling on the session path.** Plans are reconciled by a read-time hash check, not a
  filesystem watcher; sessions do not poll the network (only the skill catalog fetches, and it falls
  back to its cache).

## Layout

| Path | What it is |
|---|---|
| `crates/grove-core` | Durable core: adapters, sessions, store, spec graph, plan, skills, MCP, git. No Tauri. |
| `apps/desktop/src-tauri` | Thin Tauri shell: window, commands, event fan-out. |
| `apps/desktop/src` | React webview: panels, stores, typed IPC bridge. |
| `packages/contract` | zod schemas + TS types for the IPC contract, validated against `contract/fixtures/`. |
| `sidecar` | Node process for JS-only harness SDKs (`pi`), speaking Grove Sidecar Protocol v1. |
| `contract/fixtures` | Golden JSON written by Rust tests and parsed by TS tests — the drift alarm. |
| `fixtures/harnesses` | Fake CLIs, a real loopback HTTP+SSE server, and a sidecar host used by tests. |
| `scripts/check-bundle.mjs` | Post-build performance guard (Monaco must be a lazy chunk). |

## Local state

Grove keeps sessions, the event log, plans, pins and settings in `%APPDATA%/Grove/grove.db`. Project
state stays where it belongs: specs are repo files, skills live in each harness's own directory, and the
git repo is git. `.grove/` in a project holds per-session plan mirrors and is gitignored.

Tokens live in the OS keychain, never in the database or in `settings`.
