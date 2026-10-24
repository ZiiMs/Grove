---
id: grove-architecture
type: architecture-design
status: active
title: Grove — architecture
parent: grove-goal-and-requirements
---

# Grove — architecture

Settles the four questions `goal-and-requirements.md` left open (session persistence format,
diff/spec rendering component, the IPC contract across Rust core / Node sidecar / webview, and the
adapter interface shape), plus the repository shape the first build is cut into.

## Shape in one paragraph

A Tauri 2 application. The **webview** is a React + TypeScript SPA that renders panels and speaks one
typed IPC contract. The **Rust core** (`crates/grove-core`) owns everything durable and every external
process: adapters, sessions, SQLite, spec indexing, plans, skills, MCP, git, secrets. The **Tauri app**
(`apps/desktop/src-tauri`) is a thin shell: window, IPC command surface, event fan-out. A **Node
sidecar** exists only for harnesses whose SDK is JS-only (`pi`); it speaks newline-delimited JSON-RPC
and is replaceable by any process that speaks the same protocol.

```
┌──────────────────────────── webview (React/TS) ────────────────────────────┐
│ panels: Sessions · Skills · Specs · Plan · MCP · Git · Settings            │
│ stores (zustand)  ◄── events ──┐        ── invoke() ──┐                    │
└────────────────────────────────┼──────────────────────┼────────────────────┘
                                 │ grove:// event bus   │ Tauri commands
┌────────────────────────────────┴──────────────────────┴────────────────────┐
│ apps/desktop/src-tauri   thin shell: AppState, command modules, emit       │
└────────────────────────────────┬───────────────────────────────────────────┘
                                 │ Rust calls
┌────────────────────────────────┴───────────────────────────────────────────┐
│ crates/grove-core                                                          │
│  sessions ── adapters (claude · codex · acp · opencode · pi)               │
│  store(SQLite) · specgraph · plan · skills · mcp · gitops · secrets        │
└────────────────────────────────┬───────────────────────────────────────────┘
                                 │ stdio / HTTP+SSE / JSON-RPC
                     harness CLIs, ACP agents, opencode serve, node sidecar
```

## Repository layout

```
Cargo.toml                     workspace: crates/grove-core, apps/desktop/src-tauri
package.json                   npm workspaces: apps/desktop, sidecar, packages/contract
architecture.md                this file
goal-and-requirements.md       the goal spec
AGENTS.md                      agent skills block (tracker/domain pointers)
docs/agents/issue-tracker.md   local-markdown tracker convention
docs/agents/domain.md          domain doc layout
docs/adapters.md               "add a sixth harness" contributor guide
docs/sidecar-protocol.md       Grove Sidecar Protocol v1
packages/contract/             zod schemas + TS types + fixture validator (UI ↔ core ↔ sidecar)
contract/fixtures/             golden JSON, written by Rust tests, parsed by TS tests
crates/grove-core/src/
  adapters/{mod,registry,mock,claude,codex,acp,opencode,pi}.rs
  sessions/{mod,event,manager,pump}.rs
  store/{mod,db,repos}.rs
  specgraph/{mod,frontmatter,index,watch}.rs
  plan/{mod,service,mirror}.rs
  skills/{mod,catalog,targets,install}.rs
  mcp/{mod,discover,probe}.rs
  gitops/{mod,repo,diff,pr}.rs
  secrets/mod.rs  harness_doctor.rs  settings/mod.rs
  ipc/{mod,types,error}.rs       command DTOs + event envelope
crates/grove-core/tests/         integration tests (spawn fake harnesses)
fixtures/harnesses/*.mjs         scripted fake CLIs / fake SSE server used by tests
apps/desktop/src/                React UI
apps/desktop/src-tauri/src/
  lib.rs main.rs state.rs events.rs commands/{mod,app,sessions,plan,specs,skills,mcp,git,settings}.rs
sidecar/src/{main,rpc,pi_host}.ts
.github/workflows/ci.yml
.scratch/grove/issues/           the ticket files
```

## The adapter contract (settled)

An adapter is the **only** place a harness's protocol is known. Adapters depend on `adapters::mod`
(the contract), `sessions::event`, `ipc` DTOs and generic crates. An adapter **must not** import
another adapter, `tauri`, or any UI-facing module. The registry is a plain function, so adding a
harness is one module + one registry line + fixtures — no UI change.

```rust
pub struct HarnessId(pub &'static str); // "claude" | "codex" | "acp" | "opencode" | "pi"

#[derive(Clone, serde::Serialize)]
pub struct Capabilities {
    pub streaming_deltas: bool, pub tool_calls: bool, pub diffs: bool,
    pub permissions: bool, pub resume: bool, pub images: bool,
    pub mcp: bool, pub usage: bool,
}

pub struct StartContext {
    pub cwd: std::path::PathBuf,
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub resume_token: Option<String>,
    pub config: serde_json::Value,      // adapter-specific, from settings
    pub context_files: Vec<ContextDoc>, // pinned specs + plan, already rendered to markdown
}

pub struct AdapterDeps {
    pub paths: AppPaths,
    pub http: reqwest::Client,          // offline sessions never touch it
    pub secrets: std::sync::Arc<dyn SecretStore>,
}

#[async_trait::async_trait]
pub trait HarnessAdapter: Send + Sync {
    fn id(&self) -> HarnessId;
    fn capabilities(&self) -> Capabilities;
    fn detect(&self, deps: &AdapterDeps) -> Detection;             // installed / version / path
    async fn start(&self, ctx: StartContext, deps: AdapterDeps)
        -> Result<Box<dyn HarnessSession>, AdapterError>;
}

#[async_trait::async_trait]
pub trait HarnessSession: Send {
    fn take_events(&mut self) -> tokio::sync::mpsc::Receiver<HarnessEvent>; // once
    async fn send(&mut self, input: UserInput) -> Result<(), AdapterError>;
    async fn respond_permission(&mut self, request_id: &str, decision: Decision)
        -> Result<(), AdapterError>;
    async fn cancel(&mut self) -> Result<(), AdapterError>;
    async fn shutdown(&mut self) -> Result<(), AdapterError>;
    fn resume_token(&self) -> Option<String>;
}

pub fn builtin_registry() -> Vec<std::sync::Arc<dyn HarnessAdapter>>;
```

Adapter rules that keep them isolated:

- **Forward compatible by default.** Unknown notifications/JSON-RPC methods are logged and dropped,
  never fatal. Protocol drift is a log line, not a crashed session.
- **No panics on the wire.** Every parse error becomes `HarnessEvent::Error` or `AdapterError`.
- **Capability honesty.** A capability is `true` only when the adapter emits that event kind; the UI
  degrades from `Capabilities`, never from the harness name.
- **Explicit command override.** Each adapter reads `config.command` (and `config.args`) so a user or a
  test can point it at any binary; tests always do. Detection never blocks starting a session.

## Normalized event model (settled)

Every harness reduces to one event enum, persisted and streamed to the UI unchanged. This is the
contract that makes five harnesses one interface.

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    SessionStarted { harness: String, model: Option<String>, resumed: bool },
    Delta { text: String },
    Message { role: Role, blocks: Vec<Block> },        // Block: Text | Thinking | Code
    ToolCall { call_id: String, name: String, title: String },
    ToolResult { call_id: String, status: ToolStatus, summary: String },
    Diff { path: String, patch: String, added: u32, removed: u32 },
    PermissionRequest { request_id: String, title: String, detail: String },
    PermissionResolved { request_id: String, decision: Decision },
    Usage { input_tokens: u64, output_tokens: u64 },
    Status { state: SessionState },                    // starting|ready|busy|idle|exited|error
    Error { message: String, retryable: bool },
}

pub struct HarnessEvent { pub at: String /* RFC3339 */, pub kind: EventKind }
```

## Session runtime

`SessionManager` owns live sessions: adapter process + a pump task per session. The pump drains the
adapter's receiver, appends to the SQLite event log, broadcasts on the in-process bus, and mirrors the
resume token. Commands (`send`, `cancel`, `respond_permission`) go through the manager, which holds a
`tokio::sync::Mutex` per session so no two commands interleave on one stdio pipe.

**Restart survival.** Sessions are rows, not processes. `session_resume(id)` restarts the adapter with
the stored `resume_token` (Claude `--resume`, Codex thread id, OpenCode session id, ACP session id, pi
conversation id). If an adapter has no resume capability, Grove replays the conversation tail as
context — the log is already local, so the plan and history survive either way.

## Persistence (settled)

SQLite via `rusqlite` (`bundled`, no system dependency), one file at the Tauri app-data dir:
`%APPDATA%/Grove/grove.db`. Migrations are an ordered `&[&str]` applied under
`schema_migrations(version)`.

```sql
sessions(id TEXT PK, harness TEXT, cwd TEXT, model TEXT, title TEXT, status TEXT,
         adapter_config TEXT, resume_token TEXT, created_at TEXT, updated_at TEXT)
events(seq INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT, at TEXT,
       kind TEXT, payload TEXT)                       -- payload = full EventKind JSON
plans(session_id TEXT PK, markdown TEXT, version INTEGER, updated_at TEXT)
pins(session_id TEXT, spec_id TEXT, PRIMARY KEY(session_id, spec_id))
installs(skill TEXT, harness TEXT, path TEXT, installed_at TEXT, PRIMARY KEY(skill, harness, path))
settings(key TEXT PK, value TEXT)                     -- JSON; never secrets
```

**Files stay the source of truth where they already are.** Specs are repo `.md` files; skills live in
each harness's own directory; the git repo is git. The DB holds only what has no file home: sessions,
the event log, plan versions, pins, install records, settings.

**Plan dual-edit.** SQLite is the record; every write also mirrors `.grove/sessions/<id>/plan.md` in the
project (gitignored). A watcher imports external edits (the agent editing the file) when its hash
differs from the last mirror. Human and agent edits both move `version`, so a stale write returns
`conflict` and the UI re-reads rather than clobbering.

## IPC contract (settled)

Two channels, both typed in `packages/contract`:

1. **Commands** — `invoke(name, payload)` returning `Result<T, IpcError>`; names are
   `<domain>_<verb>` (`session_create`, `plan_set`, `spec_index`, `git_diff`, `skill_install`, …).
   Errors: `{ code: "conflict" | "not_found" | "invalid" | "unavailable" | "io", message, detail? }`.
2. **Events** — Tauri events named `grove://<domain>` with envelope `{ sessionId?, payload }`:
   `grove://session-event`, `grove://session-status`, `grove://plan-changed`,
   `grove://spec-changed`, `grove://git-changed`.

**Drift control without codegen.** `contract/fixtures/<domain>.<case>.json` holds canonical payloads.
A Rust test serialises a representative sample per DTO and asserts equality with the file; a TS test in
`packages/contract` parses the same file with its zod schema. Either side drifting breaks a test on
that side, so no build step and no generated file to conflict over. Add a fixture when you add a DTO.

Rust owns the wire shape (`serde`, `rename_all = "camelCase"` for DTOs). The webview validates incoming
event payloads with zod before storing them — an unknown event kind is ignored, not fatal.

## Sidecar protocol (settled)

`docs/sidecar-protocol.md` defines **Grove Sidecar Protocol v1**: newline-delimited JSON-RPC 2.0 over
stdio.

- `initialize {protocolVersion:1}` → `{version, harnesses:[...]}`
- `session.start {sessionId, cwd, model?, prompt?, resumeToken?, config?, context?}` → `{resumeToken?}`
- `session.send {sessionId, text}`, `session.permission {sessionId, requestId, decision}`,
  `session.cancel {sessionId}`, `session.stop {sessionId}`
- notification `session.event {sessionId, event}` — **already-normalized `HarnessEvent`**, because the
  JS SDK's shape is known only to the sidecar. Rust re-validates with serde and drops malformed ones.

The `pi` adapter is a host for this protocol, so the sidecar is swappable and the adapter is testable
against `fixtures/harnesses/pi_sidecar_fake.mjs`.

## Rendering (settled)

- **Diffs and spec editing:** Monaco (`@monaco-editor/react`), lazy-loaded on first open, diff editor
  for patches, plain editor for specs. Never mounted for anything else.
- **Spec reading and chat markdown:** `markdown-it` with HTML disabled — render, don't execute.
- **Lists:** `@tanstack/react-virtual` for the event list; panels mount only when active (state lives
  in stores), which is the memory strategy for the ~200 MB idle goal.
- **State:** `zustand` stores per domain; no global event bus in the UI beyond the IPC listener.
- **Styling:** CSS custom properties in `styles/tokens.css` + one CSS file per panel. No UI kit, no
  Tailwind — keeps the bundle and the conflict surface small.

## Subsystems

- **specgraph** — index repo `.md` files carrying frontmatter `id` + `type`; parse `parent`,
  `depends-on`, `references`, `implements`, `covers`; expose tree, ancestors, subtree, neighbours;
  validate dangling links / duplicate ids / cycles; `spec_write` is hash-guarded (optimistic
  concurrency). `notify` watcher emits `grove://spec-changed`. The index root is
  `GroveCore::project_root` (`GROVE_PROJECT_ROOT`, else the process working directory) and is returned
  as `SpecGraph.root`, so later panels share one notion of "the project".
- **plan** — see dual-edit above; `PlanService` owns version bumps and mirror writes.
- **skills** — sources are GitHub repos (default `anthropics/skills`, `mattpocock/skills`); fetch the
  repo tree, keep entries with a `SKILL.md`, cache the manifest with a TTL and serve it offline.
  `skill_targets()` lists per-harness directories (Claude `~/.claude/skills`, Codex `~/.codex/skills`,
  pi `~/.pi/skills`, OpenCode `~/.config/opencode/skills`, plus project-local `.agents/skills`).
  Install copies the folder, refuses `..` traversal and symlink escapes, records the install.
  A GitHub token is optional and lives in the keychain.
- **mcp** — discover server definitions from `.mcp.json`, `~/.claude.json`, `~/.codex/config.toml`,
  `opencode.json` and adapter config; normalise to `{name, source, transport, command/url}`. Probing
  (spawn stdio server → `initialize` + `tools/list`, or HTTP `tools/list`) is **explicit**, never
  automatic: spawning third-party servers is a side effect.
- **gitops** — shell out to `git` (no libgit2): `rev-parse --show-toplevel`, status, branches,
  worktrees, unified diff per file/branch, commit with paths, and `gh pr create --draft` returning the
  URL. Git and `gh` presence are reported by the doctor, not assumed.
- **secrets** — `keyring` crate behind `trait SecretStore` (`get/set/delete/status`); the DB only ever
  stores a key *name*. Tests use `MemorySecretStore`; one opt-in integration test touches the real
  Windows Credential Manager.
- **harness_doctor** — resolve binaries (configured path, else `PATH`), `--version`, plus node presence
  for the sidecar. Powers the Settings panel and explains an `unavailable` adapter instead of failing
  at session start.

## Extension seams (single-line conflicts, resolved at merge)

Adding a feature touches one shared list each. Keep edits to one line per file:

| Seam | File |
|---|---|
| Service wiring | `crates/grove-core/src/lib.rs` (`GroveCore` fields) |
| Commands | `apps/desktop/src-tauri/src/commands/mod.rs` (`invoke_handler!` list) |
| Adapters | `crates/grove-core/src/adapters/registry.rs` |
| UI panels | `apps/desktop/src/panels/registry.ts` |
| Dependencies | root `Cargo.toml`, root `package.json`, `apps/desktop/package.json` |

## Testing

- **Rust:** unit tests next to modules; integration tests in `crates/grove-core/tests/` spawn fake
  harnesses from `fixtures/harnesses/*.mjs` (Node is already a dependency of the build). No test talks
  to the network: `opencode`/skills/git tests point at fakes or `tempfile` repos.
- **Contract:** fixtures as described; both sides parse them.
- **TS:** `vitest` + `@testing-library/react` against a `FakeBridge` implementing the same typed bridge
  the app uses; sidecar tests drive `rpc.ts` with a fake pi runtime.
- **CI:** `.github/workflows/ci.yml`, `windows-latest`: `npm ci`, `npm run typecheck`, `npm test`,
  `npm run build`, `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`,
  `cargo test -p grove-core`, `cargo check -p grove-desktop`.
- **Not automatable here:** launching the GUI and observing webview paint. `cargo check` + `vite build`
  are the gate; a human smoke run is listed in the README.

## Windows-first, offline, security

- Windows is the build target; paths go through `AppPaths`, never string-concatenated separators.
- Sessions never require the network. Only the skill catalog fetches, and it serves cache when offline.
- Tokens/keys go to the OS keychain. `settings` JSON is asserted secret-free by test.
- No model calls are proxied by Grove — adapters launch the user's own CLI/SDK and inherit its auth.

## Open, deliberately

Issue-tracker integration in-app, cloud sync, and remote/mobile access remain out of the first build
(as the goal spec says). Protocol coverage for `codex`, `acp`, `opencode` and `pi` is validated against
recorded fixtures and is version-tolerant; a protocol change shows up as a logged warning and a
degraded capability, not a broken app.
