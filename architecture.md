---
id: grove-architecture
type: architecture-design
status: active
title: Grove — architecture
parent: grove-goal-and-requirements
---

# Grove — architecture

Settles the questions `goal-and-requirements.md` leaves open (session persistence format,
diff/spec rendering component, the IPC contract across Rust core / webview, and the provider interface
shape), plus the repository shape the build is cut into.

## Shape in one paragraph

A Tauri 2 application. The **webview** is a React + TypeScript SPA that renders the three-column
workspace and speaks one typed IPC contract. The **Rust core** (`crates/grove-core`) owns everything
durable and every external call: projects, providers, the agent runtime, sessions, SQLite, spec
indexing, plans, skills, MCP, git, secrets. The **Tauri app** (`apps/desktop/src-tauri`) is a thin
shell: window, IPC command surface, event fan-out.

There is no agent CLI and no sidecar. Grove runs the agent itself: a session picks a provider, a model
and an API key, and the runtime streams the turn and executes tools in-process. See ADR 0003.

```
┌──────────────────────────── webview (React/TS) ────────────────────────────┐
│ titlebar · project tree · chat · context (Git · Plan · Spec) · status bar  │
│ settings sheet (General · Theme · Skills · Providers · MCP)                │
│ workspace stores (zustand)  ◄── events ──┐   ── invoke() ──┐               │
└────────────────────────────────────────────┼─────────────────┼─────────────┘
                                 │ grove:// event bus   │ Tauri commands
┌────────────────────────────────┴──────────────────────┴────────────────────┐
│ apps/desktop/src-tauri   thin shell: AppState, command modules, emit       │
└────────────────────────────────┬───────────────────────────────────────────┘
                                 │ Rust calls
┌────────────────────────────────┴───────────────────────────────────────────┐
│ crates/grove-core                                                          │
│  sessions ── providers (five vendors, see catalog.rs)                      │
│  store(SQLite) · specgraph · plan · skills · mcp · gitops · secrets        │
└────────────────────────────────┬───────────────────────────────────────────┘
                                 │ HTTPS + SSE
                        vendor APIs (Anthropic · OpenAI · Google · OpenRouter · OpenCode Go)
```

## Repository layout

```
Cargo.toml                     workspace: crates/grove-core, apps/desktop/src-tauri
package.json                   npm workspaces: apps/desktop, packages/contract
architecture.md                this file
goal-and-requirements.md       the goal spec
AGENTS.md                      agent guide (tracker/domain pointers)
GLOSSARY.md                    the terms this codebase actually uses
docs/agents/issue-tracker.md   local-markdown tracker convention
docs/agents/domain.md          domain doc layout
docs/adr/                      architecture decision records (decisions that changed during the build)
scripts/check-bundle.mjs       bundle guard run after `npm run build`
packages/contract/             zod schemas + TS types + fixture validator (UI ↔ core)
contract/fixtures/             golden JSON, written by Rust tests, parsed by TS tests
crates/grove-core/src/
  providers/{mod,catalog,credentials,wire,anthropic,openai,google,probe,tools,agent,offline}.rs
  sessions/{mod,event,manager,pump,context,title}.rs
  projects/mod.rs                 workspace folders + folder browsing
  reveal.rs                       platform file-manager reveal for a path
  store/{mod,db,repos,plans,pins,projects}.rs
  specgraph/{mod,frontmatter,index,watch}.rs
  plan/{mod,service,mirror}.rs
  skills/{mod,catalog,targets,install}.rs
  mcp/{mod,discover,probe}.rs
  gitops/{mod,types,repo,diff,pr}.rs
  secrets/mod.rs  settings/mod.rs
  ipc/{mod,types,error}.rs       command DTOs + event envelope
crates/grove-core/tests/         integration tests (wiremock vendor fakes, tempfile projects)
apps/desktop/src/                React UI: shell/, session/, context/, settings/, dialogs/, workspace/, state/
apps/desktop/src/styles/         themes.css (token contract) · design.css (shell) · extras.css · fonts/
apps/desktop/tools/              visual screenshot harness (not part of `npm test`)
apps/desktop/src-tauri/src/
  lib.rs main.rs state.rs events.rs commands/{mod,app,projects,sessions,plan,specs,skills,mcp,git,settings}.rs
.scratch/grove/issues/           first-build tickets (history; new work is in ClickUp)
```

## The provider contract (settled)

A provider module is the **only** place one vendor's HTTP API is known. Providers depend on
`providers::mod` (the contract), `sessions::event`, the IPC DTOs and generic crates. A provider
**must not** import another provider, `tauri`, or any UI-facing module. The catalog is a plain
function, so adding a vendor is one module + one catalog entry + fixtures — no UI change.

```rust
pub struct ProviderSpec {                 // what the picker and settings pane read
    pub id: String, pub name: String, pub short: String, pub tint: String,
    pub wire: Wire,                       // Anthropic | OpenAi | Google
    pub base_url: String,
    pub auth: AuthMethod,                 // ApiKey | SignIn
    pub key_url: String, pub env_var: String,
    pub models: Vec<String>, pub default_model: String,
    pub capabilities: Capabilities,
}

pub struct StartContext {
    pub cwd: PathBuf, pub model: Option<String>, pub session_id: String,
    pub prompt: Option<String>,   // replayed transcript tail (resume fallback)
    pub history: Vec<wire::Message>, // full conversation, when a live session is handed over
    pub resume_token: Option<String>,
    pub config: serde_json::Value, // `maxSteps`, `thinkingLevel`, `permissionMode`
    pub context_files: Vec<ContextDoc>, // pinned specs + plan, as markdown
}

pub struct ProviderDeps { pub paths: AppPaths, pub http: reqwest::Client,
                          pub secrets: Arc<dyn SecretStore> }

#[async_trait::async_trait]
pub trait AgentProvider: Send + Sync {
    fn id(&self) -> &str;
    fn spec(&self) -> &ProviderSpec;
    fn capabilities(&self) -> Capabilities;
    fn detect(&self, deps: &ProviderDeps) -> Detection;   // is a credential configured?
    async fn complete(&self, req: CompletionRequest, deps: ProviderDeps)
        -> Result<String, ProviderError>;                // one-shot, no tools or history
    async fn start(&self, ctx: StartContext, deps: ProviderDeps)
        -> Result<Box<dyn AgentSession>, ProviderError>;
}

#[async_trait::async_trait]
pub trait AgentSession: Send {
    fn take_events(&mut self) -> tokio::sync::mpsc::Receiver<SessionEvent>;  // once
    async fn send(&mut self, input: UserInput) -> Result<(), ProviderError>;
    async fn history(&mut self) -> Vec<wire::Message>;  // [] = keep the transcript replay
    async fn respond_permission(&mut self, request_id: &str, decision: Decision)
        -> Result<(), ProviderError>;
    async fn set_thinking(&mut self, level: ThinkingLevel) -> Result<(), ProviderError>;
    async fn set_model(&mut self, model: String) -> Result<(), ProviderError>;
    async fn cancel(&mut self) -> Result<(), ProviderError>;
    async fn shutdown(&mut self) -> Result<(), ProviderError>;
    fn resume_token(&self) -> Option<String>;
    fn resume_token_slot(&self) -> Option<ResumeToken> { None }
}

pub fn registry() -> Vec<Arc<dyn AgentProvider>>;   // providers::agent
```

Provider rules that keep them isolated:

- **One wire format, one client.** `anthropic.rs`, `openai.rs` and `google.rs` each own request
  encoding and stream decoding behind `wire::StreamDecoder`. OpenAI, OpenRouter and OpenCode Go share
  the OpenAI client and differ only by catalog entry.
- **Gateway extras live with the catalog.** A provider that needs a non-default model-list URL or a
  conversation header declares it in `catalog::transport(id)`; the runtime applies it generically, so
  `ProviderSpec` keeps the shape the picker reads and no contract field is added for one vendor.
- **Model lists are live, the catalog is the floor.** `provider_models` asks the vendor which models
  the configured key can reach. After boot loads provider statuses, the webview refreshes each
  configured provider and replaces that provider's `models` (and moves `defaultModel` if its preset is
  gone); a failed probe leaves the catalog list in place. The whole currency is the API's own list, so
  a newly released model appears without a Grove release and an unavailable preset does not trap a new
  session. The request is only made after boot or a key change, never on a session turn.
- **Thinking is a session preference, not a prompt trick.** `ThinkingLevel` lives in
  `sessions.adapter_config` as `thinkingLevel` and is surfaced as `SessionDetail.thinkingLevel`.
  `session_set_thinking` updates the live agent's next turn and persists the config for a stopped
  session.
- **Approval mode is the same kind of preference.** `PermissionMode` lives in `adapter_config` as
  `permissionMode` and is surfaced as `SessionDetail.permissionMode`. `ask` opens a permission card
  before `write_file`, `edit_file` or `run_command`; `full` runs them without asking, while `ask_user`
  questions still reach the user. `session_set_permission_mode` applies to a live agent from its next
  tool call and persists for a stopped one. The selectable values are per-model, not global: `providers::model_catalog` reads the
  public models.dev catalogue's provider-specific `reasoning_options` into a one-hour file cache and
  `provider_thinking` hands the result to the webview. A model can therefore expose `off`/`none`,
  `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, or the static fallback when metadata is absent.
  Each wire module maps the selected value to the vendor's native knob: Anthropic
  `thinking.budget_tokens`, OpenAI/OpenRouter/OpenCode Go `reasoning_effort`, and Google
  `generationConfig.thinkingConfig.thinkingBudget`; `off` omits the field and `none` is the explicit
  disabled value for models that require one.
- **Forward compatible by default.** Unknown stream frames are dropped, not fatal: a vendor adding an
  event must degrade to a missing detail, never a dead session.
- **No panics on the wire.** Every parse error becomes `ProviderError`, which the runtime turns into a
  transcript `error` row.
- **Capability honesty.** A capability is `true` only when the runtime really emits that event kind;
  the UI degrades from `Capabilities`, never from the provider name.
- **Credentials fail fast.** `start` resolves the key before the first request, so a missing key is a
  clear `unavailable` at launch instead of an error after the user typed a message.
- **One copy of each helper.** `providers/wire.rs` owns SSE framing, auth headers and API-error
  wording, so a fix to the header shape cannot miss a vendor.

## The agent runtime (settled)

`providers/agent.rs` runs one conversation. A turn is spawned rather than awaited inside
[`AgentSession::send`], because the session manager serializes commands with a mutex: a turn that ran
under that lock would make `cancel` unreachable. Turns are still serialized against each other by the
session's own gate.

Ordering is the transcript's contract:

```
status busy → delta* → message(assistant) → tool_call → permission_request →
  permission_resolved → tool_result → diff? → (next turn) → usage → status idle
```

- **Tools** (`providers/tools.rs`): `read_file`, `list_dir`, `grep`, `write_file`, `edit_file`,
  `run_command`, `ask_user`, `subagent`. Paths resolve against the session's working directory; `..`,
  an absolute path outside it, and a symlink escape are refused.
- **Approval** is a property of the tool name, not something the model can influence.
  `needs_approval` covers the three mutating tools; the runtime parks the call on a `oneshot` channel
  until `respond_permission` resolves it, and a denied call becomes an error tool result the model
  reads and adapts to. `ask_user` is a separate parked call: it emits `question_request`, renders as
  an inline card, and returns the structured answer as its tool result.
- **Subagents are worker turns.** `subagent` starts another `AgentSessionImpl` with the same provider,
  model, key, project directory and pinned context, but a fresh conversation and every tool except
  `subagent`, so it cannot recurse. Its mutating calls still ask the parent's user: the worker's
  `permission_request` and `question_request` are re-emitted on the parent's stream under `sub-` ids
  and the answers are relayed back. Its tool rows and diffs are forwarded with the same prefix; its
  deltas, messages, status and errors stay inside the worker. The parent receives one tool result: the
  worker's final report, capped and marked when truncated. The pump records the call in `subagents`
  (description, status, result summary), so the session tree lists the worker without reading the
  transcript; selecting it opens a read-only view of the worker's forwarded rows, with its question
  and approval cards still answerable. A turn that ends while a worker is still running closes it as
  cancelled.
- **The loop is bounded** by `maxSteps` (default 24, from `StartContext.config`), so a model that
  never converges stops and says so instead of spending indefinitely.
- **Cancellation** aborts the turn task, denies every parked approval, resolves every parked question
  as empty, and emits `status idle`, so the session is never left `busy`.

## Normalized event model (settled)

Every provider reduces to one event enum, persisted and streamed to the UI unchanged. This is the
contract that makes five vendors one interface.

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    SessionStarted { provider: String, model: Option<String>, resumed: bool },
    Delta { text: String },
    Message { role: Role, blocks: Vec<Block> },        // Block: Text | Thinking | Code
    ToolCall { call_id: String, name: String, title: String },
    ToolResult { call_id: String, status: ToolStatus, summary: String },
    Diff { path: String, patch: String, added: u32, removed: u32 },
    PermissionRequest { request_id: String, title: String, detail: String },
    PermissionResolved { request_id: String, decision: Decision },
    QuestionRequest { request_id: String, questions: Vec<Question> },
    QuestionResolved { request_id: String, answers: Vec<QuestionAnswer> },
    Usage { input_tokens: u64, output_tokens: u64 },
    Status { state: SessionState },                    // starting|ready|busy|idle|exited|error
    TitleChanged { title: String, source: TitleSource }, // auto|generated|manual
    Error { message: String, retryable: bool },
    ContextRefreshed { reason: ContextReason, provider: String,
                       documents: Vec<ContextDocument>, note: String },
}

pub struct SessionEvent { pub at: String /* RFC3339 */, pub kind: EventKind }
```

## Session runtime

`SessionManager` owns live sessions: the agent handle + a pump task per session. The pump drains the
agent's receiver, appends to the SQLite event log, broadcasts on the in-process bus, and mirrors the
resume token — watching the runtime's shared slot, so an id learned mid-stream is persisted. Commands
(`send`, `cancel`, `respond_permission`, `respond_question`) go through the manager, which holds a
`tokio::sync::Mutex` per session so no two commands interleave.

**Answering a permission request.** The runtime emits `PermissionRequest` and claims
`capabilities.permissions`, so the request has to be answerable: it renders as a card in the transcript
with Approve/Deny, and those buttons call `session_respond_permission(id, requestId, decision)` →
`SessionManager::respond_permission` → the parked tool call. The buttons disappear when
`permission_resolved` arrives, not when the click happens: what the UI shows is what the runtime
accepted.

**Answering a question.** `ask_user` emits `QuestionRequest` with one or more questions and parks the
tool call on a `oneshot` channel. The transcript renders one inline card that pages through them,
accepts option choices and optional notes, and calls `session_respond_question(id, requestId,
answers)` → `SessionManager::respond_question`. The runtime emits `question_resolved` and hands the
answers back to the model as the tool result. A cancelled turn resolves any parked question as empty,
so the card cannot stay interactive after the agent has gone.

**Restart survival.** Sessions are rows, not processes. `session_resume(id)` restarts the agent with
the stored `resume_token` when the provider offers one, otherwise Grove replays the conversation tail
as context — the log is already local, so the plan and history survive either way. A stopped session
resumes implicitly when its next message is sent; the composer has no separate Resume action, so the
user continues the conversation instead of managing its process state.

**Session knowledge.** Start, resume and provider switch assemble `StartContext.context_files` from
the session's pinned specs (in pin order) followed by its current plan, read fresh from the project's
spec files and `PlanService`. `sessions/context.rs` enforces a per-document and a total character
budget and appends an explicit `[truncated: <n> of <total> chars omitted]` marker to any document it
cuts; a document that cannot fit is dropped and counted. The assembled context is a **snapshot**:
nothing re-reads a spec on a timer, so an edit cannot silently change a running conversation.

Two runtime operations move it, and both are visible in the transcript as a `ContextRefreshed` note:

- `session_refresh_context(id)` re-reads pins and plan, pushes the rendered documents into the **running**
  session as a turn, and records what was sent.
- `session_switch_provider(id, provider, model)` re-launches the same conversation on another registered
  provider. A live, idle session hands over its normalized history, trimmed to the last complete
  assistant message, so the transcript is not replayed; a stopped session falls back to a transcript
  summary (`render_tail`). The switch keeps the transcript and drops the old resume token and model.
  While the session is busy or waiting for a permission/question it is refused with a `conflict` and
  the shell queues it until idle (ADR 0005).

`session_set_thinking(id, level)` is not a context operation: it persists `thinkingLevel` in the
session config and updates a live agent in place; a stopped session picks it up on resume. It does not
relaunch or touch the transcript.

The manager talks to knowledge through the `ContextSource` port, so it never imports the spec graph or
the plan service; `GroveCore::assemble` wires the real `SessionContextService` onto `GroveCore::context`.
Pins live in the `pins` table, are toggled from the Specs panel for the session the Sessions panel has
open, and survive a restart.

**One notion of "the project".** `GroveCore::project_root` is a `ProjectRoot` — a shared
`Arc<RwLock<PathBuf>>` — and a session's `cwd` decides it: the shell calls `app_set_project_root` (or
`commands::app::adopt`, which also re-targets the spec watcher and emits `grove://spec-changed`) when a
session is created, resumed, or its tab is activated. The spec graph, the plan mirror's fallback, MCP
discovery and the project-local skills target all hold a clone of that one handle, so nothing caches a
stale project and no service has to be rebuilt. `GROVE_PROJECT_ROOT` (else the process cwd) is only the
*initial* value, which keeps headless runs and tests pinnable.

## Persistence (settled)

SQLite via `rusqlite` (`bundled`, no system dependency), one file at the Tauri app-data dir:
`%APPDATA%/Grove/grove.db`. Migrations are an ordered `&[&str]` applied under
`schema_migrations(version)`; later domains append rather than edit an earlier migration, so an
existing install upgrades in place. Ten migrations have shipped:

| # | Tables / columns | Feature |
|---|---|---|
| 1 | `sessions`, `events`, `events_by_session` | session runtime |
| 2 | `plans` (+ `mirror_hash`) | TODO plan and mirror |
| 3 | `settings` | typed settings documents |
| 4 | `pins` | specs pinned to a session |
| 5 | `projects`, `sessions.project_id`, `sessions.awaiting` | workspace tree and "needs you" |
| 6 | `sessions.provider` (renamed from `harness`) | providers replace harnesses (ADR 0003) |
| 7 | `subagents` (+ backfill from `tool_call`/`tool_result`) | durable delegated-worker trace |
| 8 | `sessions.archived_at`, `sessions_by_archive` | archive, restore and permanent delete |
| 9 | `sessions.title_source` | optional session titles and first-message placeholders |
| 10 | `sessions.pinned` | pinned sessions in the sidebar |

```sql
projects(id TEXT PK, name TEXT UNIQUE, path TEXT UNIQUE, remote TEXT, default_branch TEXT,
         is_git INTEGER, created_at TEXT, updated_at TEXT)
sessions(id TEXT PK, provider TEXT, cwd TEXT, model TEXT, title TEXT, status TEXT,
         adapter_config TEXT, resume_token TEXT, project_id TEXT REFERENCES projects(id),
         awaiting INTEGER NOT NULL DEFAULT 0, archived_at TEXT,
         title_source TEXT NOT NULL DEFAULT 'manual', pinned INTEGER NOT NULL DEFAULT 0,
         created_at TEXT, updated_at TEXT)
events(seq INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT, at TEXT,
       kind TEXT, payload TEXT)                       -- payload = full EventKind JSON
plans(session_id TEXT PK, markdown TEXT, version INTEGER, mirror_hash TEXT, updated_at TEXT)
pins(session_id TEXT, spec_id TEXT, PRIMARY KEY(session_id, spec_id))
settings(key TEXT PK, value TEXT)                     -- JSON; never secrets
```

`projects` holds only what cannot be read back from disk; worktrees are read from `git worktree list`
on demand, because git owns checkout state. `awaiting` is mirrored by the pump from permission events,
so the tree and titlebar can say "needs you" without reading a transcript.

**Files stay the source of truth where they already are.** Specs are repo `.md` files; the git repo is
git. The DB holds only what has no file home: sessions, the event log, plan versions, pins, settings,
and project rows. Provider API keys live in the OS keychain, never here. Skill installs are a JSON file
under the app-data dir behind the `InstallStore` trait.

**Plan dual-edit.** SQLite is the record; every write also mirrors `.grove/sessions/<id>/plan.md` in the
project (gitignored), atomically (temp file + rename). `mirror_hash` holds the content the record last
wrote to that file, so a read can tell the agent's external edit apart from Grove's own mirror write.
Before every read (and so before every save) `PlanService` compares the file's hash with the record and
the mirrored hash: a file that changed while the record stayed put is imported (version bumped, record
written, `grove://plan-changed` emitted); a file that is stale or missing is rewritten from the record.
The Plan panel also re-reads on window focus and on a short timer, which is what makes an agent edit
visible without a native watcher. Human and agent edits both move `version`, so a stale write returns
`conflict` and the UI re-reads rather than clobbering.

## IPC contract (settled)

Two channels, both typed in `packages/contract`:

1. **Commands** — `invoke(name, payload)` returning `Result<T, IpcError>`; names are
   `<domain>_<verb>` (`session_create`, `plan_set`, `spec_index`, `git_diff`, `skill_install`,
   `provider_key_set`, …). Errors: `{ code: "conflict" | "not_found" | "invalid" | "unavailable" | "io",
   message, detail? }`.
2. **Events** — Tauri events named `grove://<domain>` with envelope `{ sessionId?, payload }`:
   `grove://session-event`, `grove://plan-changed`, `grove://spec-changed`, `grove://git-changed`.
   There is deliberately no separate status channel: a session's state rides in the `status` event kind
   inside `grove://session-event`, so there is one stream to persist, order and replay. Every channel
   above has a subscriber in `apps/desktop/src/ipc/events.ts` — an event nobody listens to is a bug.

**App/window commands.** `app_window_minimize`, `app_window_toggle_maximize`,
`app_window_is_maximized` and `app_window_close` drive the custom titlebar; `app_info` and
`app_set_project_root` carry the rest of the shell state.

**Provider commands.** `provider_list` (catalogue), `provider_statuses` (credential readiness),
`provider_key_set` / `provider_key_delete` (keychain writes; neither returns a value) and
`provider_models` (asks the vendor which models the key can reach). `provider_models` powers both the
settings pane's "test" button and the picker refresh that runs after boot; `provider_list` remains the
offline fallback. `session_set_thinking` changes the persisted reasoning level for one session.
`session_set_pinned` pins a session into the sidebar's Pinned section; it is a preference and does not
touch `updated_at`.
`plan_usage` reports the remaining 5-hour/weekly/monthly budget for a metered provider (OpenCode Go's
`/zen/go/v1/usage`); it returns `null` for providers without a plan endpoint and never runs on a
session turn.

**Session titles.** `settings.titleModel` is the one global small model that names sessions, or null
when the suggested model applies; `settings.autoTitles` is the Off switch, because clearing the model
alone means "use the suggestion", not "never name". `clearTitleModel` is how the patch expresses a
cleared model, because a JSON null cannot be told apart from an omitted field. `title_model_suggest`
answers the suggestion for the first configured provider (the shell passes its live model lists so the
OpenRouter choice can follow the account) and never writes settings. `session_generate_title` runs one
non-streaming completion with no tools and no history, then stores the title as `generated` and emits
`title_changed`; its `force` flag separates the explicit menu action from the automatic trigger, which
never overwrites a manual title. See ADR 0004.

**Drift control without codegen.** `contract/fixtures/<domain>.<case>.json` holds canonical payloads.
A Rust test serialises a representative sample per DTO and asserts equality with the file; a TS test in
`packages/contract` parses the same file with its zod schema. Either side drifting breaks a test on
that side, so no build step and no generated file to conflict over. Add a fixture when you add a DTO.

Rust owns the wire shape (`serde`, `rename_all = "camelCase"` for DTOs). The webview validates incoming
event payloads with zod before storing them — an unknown event kind is ignored, not fatal.

Command rejections are normalised once, in `apps/desktop/src/ipc/errors.ts`: the wire
`{ code, message, detail? }`, a plain `Error`, or an unknown value all become
`{ code, message, detail }` with a message that is never empty. Panels print `errorText(error)` and Git
renders the code beside the message; no panel formats a rejection itself.

## Rendering (settled)

The shipped shell is the three-column workspace from `design-reference/`; ADR 0002 records why the
seven-panel shell and the spec/plan editors were retired.

- **Shell:** custom titlebar · resizable project tree (300px by default, dragged from its right edge) · always-mounted chat · 304px context column · floating session-info card
  (Git · Plan · Spec) · status bar. Below 900px the three columns become Navigation · Session ·
  Context tabs. The centre never swaps for another product surface.
- **Window chrome:** the native decorations are off. The titlebar's marked drag regions move the
  window; its minimize, maximize/restore and close buttons call the typed `app_window_*` commands, so
  the webview still never imports Tauri's window API and the controls remain fakeable. New sessions
  start from the `+` actions in the tree, not from the titlebar.
- **Transcript:** `@tanstack/react-virtual` mounts only the rows in view, so a 10 000-event session
  costs the same as a short one. Tool rows, diffs, permission cards, and inline question cards are
  transcript rows, not separate panels.
- **Context views are read-only:** git status/staging/commit/PR, a plan checklist, and a spec peek.
  There is no in-app spec or plan editor and therefore no code editor in the webview.
- **Dialogs:** native `<dialog>` with `showModal()` for the settings sheet, new session, add project,
  diff and pull request, so focus trapping and Escape come from the platform.
- **Pickers:** `shell/Combo.tsx` is the one listbox control. Model pickers search across grouped
  provider lists; thinking pickers use the same control with search disabled. The popup supports
  arrow keys, Enter, Escape and focus-out dismissal.
- **Notifications:** one status line in the bottom-right, above the status bar; a newer message
  replaces the current one.
- **Build guard:** `scripts/check-bundle.mjs` runs after `npm run build`, fails if Monaco reappears in
  the entry chunk or the entry chunk exceeds 600 kB, and reports the largest lazy chunks.
- **State:** `zustand` stores per domain (`sessionStore` for the transcript, `workspaceStore` for
  projects/preferences/theme, `contextStore` for git/plan/specs, `skillsStore`, `uiStore`); no global
  event bus beyond the IPC listener.
- **Styling:** the exported token contract in `styles/themes.css` (seven themes × dark/light) plus
  `styles/design.css` for the shell. Components consume tokens only; no UI kit, no Tailwind. Theme
  fonts are vendored latin subsets, and the saved theme is applied before first paint.

## Subsystems

- **specgraph** — index repo `.md` files carrying frontmatter `id` + `type`; parse `parent`,
  `depends-on`, `references`, `implements`, `covers`; expose tree, ancestors, subtree, neighbours;
  validate dangling links / duplicate ids / cycles; `spec_write` is hash-guarded (optimistic
  concurrency). `notify` watcher emits `grove://spec-changed`, which the Specs panel subscribes to and
  answers with a debounced re-index (one save costs one pass, a burst costs one pass). The Git panel
  reads a pinned spec beside the diff, so the change and the intent that asked for it are in one view.
  The index root is `GroveCore::project_root`, returned as `SpecGraph.root`, so every panel shares one
  notion of "the project".
- **plan** — see dual-edit above; `PlanService` owns version bumps and mirror writes.
- **skills** — sources are GitHub repos (default `anthropics/skills`, `mattpocock/skills`); fetch the
  repo tree, keep entries with a `SKILL.md`, cache the manifest with a TTL and serve it offline. The
  source list persists in `settings.catalogSources`, and the Skills pane can append an `owner/repo`
  (or a pasted GitHub URL).
  `skill_targets()` lists the two places Grove's own agent reads a skill from: the user's
  `~/.agents/skills` and the project-local `.agents/skills`. `skill_inventory()` scans both for
  folders containing a `SKILL.md`, so a skill another tool placed there still reads as installed.
  Install copies the folder, refuses `..`
  traversal and symlink escapes, records the install, and removal deletes only a folder under a
  known target that contains a `SKILL.md` — recorded or not. A GitHub token is optional: the
  keychain is checked first, then `GITHUB_TOKEN` / `GH_TOKEN`, then `gh auth token`. Discovered
  tokens are used in memory only and never written back.
- **mcp** — discover server definitions from `.mcp.json`, `opencode.json` and provider config;
  normalise to `{name, source, transport, command/url}`. Probing (spawn stdio server → `initialize` +
  `tools/list`, or HTTP `tools/list`) is **explicit**, never automatic: spawning third-party servers is
  a side effect.
- **projects** — a folder the user added, with the git facts detected at add time (`is_git`,
  `default_branch`, normalized `owner/name` remote). `fs_list_dirs` lists one level for the picker;
  `fs_list_roots` adds home and every mounted drive/root, and `fs_create_dir` creates one validated
  segment under the current folder. `project_remove` deletes the projects row only: its sessions become
  chats (`project_id` is `ON DELETE SET NULL`) and the working tree is untouched. Adding a project
  backfills `sessions.project_id` for conversations that already ran in the folder or one of its
  worktrees. Worktrees themselves stay in git.
- **gitops** — shell out to `git` (no libgit2): `rev-parse --show-toplevel`, status, branches,
  worktrees, unified diff per file/branch, stage/unstage selected paths, commit with paths, create a
  branch plus worktree, and `gh pr create` (draft or ready) returning the URL.
- **secrets** — `keyring` crate behind `trait SecretStore` (`get/set/delete/status`); the DB only ever
  stores a key *name*. Tests use `MemorySecretStore`; one opt-in integration test touches the real
  Windows Credential Manager.
- **providers** — the catalog, credentials, the three wire clients, the tool sandbox and the agent
  runtime described above.

## Extension seams (single-line conflicts, resolved at merge)

Adding a feature touches one shared list each. Keep edits to one line per file. A provider needs one
more: `crates/grove-core/src/providers/catalog.rs` is authoritative for the fallback models, auth and
default model, while `apps/desktop/src/workspace/models.ts` holds only a presentation fallback (mark,
tint, known models) so a row renders before the catalogue has loaded; an unknown id degrades to its
initials. The live list from `provider_models` overrides the fallback once a probe succeeds.

| Seam | File |
|---|---|
| Service wiring | `crates/grove-core/src/lib.rs` (`GroveCore` fields) |
| Commands | `apps/desktop/src-tauri/src/commands/mod.rs` (`invoke_handler!` list) |
| Providers | `crates/grove-core/src/providers/catalog.rs` |
| UI surfaces | `apps/desktop/src/App.tsx` (titlebar, tree, chat, context, status, dialogs) |
| Dependencies | root `Cargo.toml`, root `package.json`, `apps/desktop/package.json` |

## Testing

- **Rust:** unit tests next to modules; integration tests in `crates/grove-core/tests/`. No test talks
  to a real vendor: the wire clients are exercised against `wiremock` servers and recorded frames, and
  the agent loop against the offline provider. `adapters_*`-style conformance suites are gone with the
  adapters; `session_runtime.rs`, `session_restart.rs` and `session_context.rs` pin the runtime,
  restart and knowledge behaviours against the offline provider.
- **Contract:** fixtures as described; both sides parse them.
- **TS:** `vitest` + `@testing-library/react` against a `FakeBridge` implementing the same typed bridge
  the app uses. `apps/desktop/tools/visual.test.tsx` (not in `npm test`) renders the built shell into
  `dist/visual-*.html` for screenshot comparison across the viewport matrix.
- **No CI.** The gates are `npm ci`, `npm run typecheck`, `npm test`, `npm run build` (which ends with
  the bundle guard), `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test -p grove-core` and `cargo check -p grove-desktop`. Nothing runs them
  automatically, so the change that lands is responsible for having run them.
- **Not automatable here:** launching the GUI and observing webview paint. `cargo check` + `vite build`
  are the gate; a human smoke run is listed in the README.

## Windows-first, offline, security

- Windows is the build target; paths go through `AppPaths`, never string-concatenated separators.
- A session **is** network traffic: it calls the provider the user configured. Outside a turn, the
  skill catalog, the live provider model refresh, and the public model-capability catalogue fetch; each
  serves a cache or static fallback when offline.
- Keys go to the OS keychain. `settings` JSON is asserted secret-free by test, and no command returns a
  key value.
- Grove sends the model request directly with the user's key. There is no Grove service in the middle,
  no proxy and no account.

## Open, deliberately

Issue-tracker integration in-app, cloud sync, and remote/mobile access remain out of the first build.
Account sign-in is offered by one vendor: OpenRouter documents "Sign In with OpenRouter" (a PKCE
flow for third-party apps), while Anthropic, OpenAI and Google restrict subscription OAuth to their
own first-party clients. This build ships API keys for all five; the OpenRouter sign-in flow is the
documented next step (ADR 0003). `AuthMethod` already carries `SignIn` and `ProviderSpec::auth`
drives the settings row, so adding it is not a UI change.
