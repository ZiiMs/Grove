# Writing a harness adapter

Grove's promise is one interface for every AI coding harness. An adapter is the **only** place one
harness's protocol is known: it starts the harness, translates its wire format into the normalized
event model, and turns user actions back into the harness's own commands. Everything else — the
session runtime, SQLite transcript, event bus, webview — is shared.

Adding a sixth harness touches **no UI code**. The picker renders whatever `harness_list` returns, and
the transcript renders normalized events, so a new adapter appears in the app as soon as it is
registered and detected.

Authoritative context: `architecture.md` §The adapter contract, §Normalized event model, and
§Extension seams. This guide is the practical companion.

## Layout

| Path | What lives there |
|---|---|
| `crates/grove-core/src/adapters/mod.rs` | The contract: `HarnessAdapter`, `HarnessSession`, `Capabilities`, `StartContext`, `AdapterDeps`, `Detection`, `AdapterError`, `HarnessId` |
| `crates/grove-core/src/adapters/<harness>.rs` | One adapter, its process/actor, and its line-by-line mapping |
| `crates/grove-core/src/adapters/support.rs` | The helpers every process-backed adapter shares: `find_on_path`, `render_prompt`, `truncate`, `command_from_config`, `kill_tree_by_pid`, `kill_process_tree`. An adapter calls these instead of carrying its own copy |
| `crates/grove-core/src/adapters/registry.rs` | `builtin_registry()` — the one line that makes an adapter real |
| `crates/grove-core/src/adapters/mock.rs` | The in-process reference adapter ("does Grove itself work?") |
| `fixtures/harnesses/<harness>_fake.mjs` | A scripted stand-in for the real CLI, used by every test |
| `crates/grove-core/tests/adapters_<harness>.rs` | One test per fixture mode |
| `crates/grove-core/tests/adapters_conformance.rs` | The shared suite, run against every registered adapter |
| `contract/fixtures/*.json` | Golden payloads asserted from Rust **and** TypeScript |

## The contract

```rust
pub trait HarnessAdapter: Send + Sync {
    fn id(&self) -> HarnessId;                                  // "claude" | "codex" | …
    fn display_name(&self) -> &'static str { self.id().as_str() } // "Claude Code", "Codex", …
    fn skills_dir(&self) -> Option<&'static str> { None }       // ".claude/skills", … or None
    fn capabilities(&self) -> Capabilities;                     // honest, never aspirational
    fn detect(&self, deps: &AdapterDeps) -> Detection;          // installed / version / path
    async fn start(&self, ctx: StartContext, deps: AdapterDeps)
        -> Result<Box<dyn HarnessSession>, AdapterError>;
}

pub trait HarnessSession: Send {
    fn take_events(&mut self) -> mpsc::Receiver<HarnessEvent>;  // called exactly once
    async fn send(&mut self, input: UserInput) -> Result<(), AdapterError>;
    async fn respond_permission(&mut self, request_id: &str, decision: Decision)
        -> Result<(), AdapterError>;
    async fn cancel(&mut self) -> Result<(), AdapterError>;
    async fn shutdown(&mut self) -> Result<(), AdapterError>;
    fn resume_token(&self) -> Option<String>;
    fn resume_token_slot(&self) -> Option<ResumeToken> { None }   // defaulted
}
```

`StartContext` carries everything a launch needs: `cwd`, `model`, an optional `prompt`, an optional
`resume_token`, `config` (adapter-specific JSON from settings), and `context_files` (pinned specs and
the plan, already rendered to markdown). `AdapterDeps` exposes paths, an HTTP client, and the secret
store; **a session must never call the network** — the harness inherits the user's own auth.

`display_name()` and `skills_dir()` are the two facts about a harness that are not protocol:
the doctor and the Skills dropdown are both built from `builtin_registry()`, so a new adapter shows up
in them by overriding these (or by leaving the defaults, which is fine for a harness with no skills
directory). No `match` on harness ids exists outside the adapters.

**Where `config` comes from.** The session manager fills `config.command` from
`Settings.harness_paths[harness]` when the request carries none, through the `AdapterConfigSource` port;
an explicit request `config.command` always wins and every other key is passed through untouched. That
is why an adapter must read the command with `support::command_from_config` and never hard-code its
binary: a user's configured path, and the ACP adapter's only way to start at all, arrive that way.

Rules that keep adapters isolated and Grove trustworthy:

- **Forward compatible by default.** An unknown line, event type, or JSON-RPC method is logged and
  dropped. Protocol drift is a log line, not a dead session.
- **No panics on the wire.** Malformed input becomes a logged skip, an `EventKind::Error`, or an
  `AdapterError` — never a panic.
- **Capability honesty.** A flag is `true` only if the adapter really emits that event kind for the
  launch shape Grove uses. The UI degrades from `Capabilities`, never from the harness name.
- **Explicit command override.** Read `config.command` and `config.args` with
  `support::command_from_config`. A user points Grove at a wrapper (a path from Settings arrives here
  too), and tests point it at a fixture.
- **Detection never blocks starting.** `detect()` is side-effect free (resolve on `PATH`, no probes)
  and its result does not gate `start()`. A missing binary becomes an error *event* on the first
  turn, so the session row and transcript still exist; the doctor explains it in Settings.
- **One writer per session.** The session manager serializes `send`/`cancel`/`respond_permission`
  behind a per-session mutex. An adapter's actor must still never run two harness processes against
  one conversation: queue prompts and run them in order.
- **Answer every permission request.** A `PermissionRequest` reaches the transcript as a card with
  Approve/Deny; the answer arrives as `respond_permission(request_id, decision)` from the
  `session_respond_permission` command. Emit the matching `PermissionResolved` when the protocol
  confirms it — that event is what removes the buttons — and never leave a request unanswered: an
  agent waiting on one hangs the turn. An adapter that cannot answer must leave
  `capabilities.permissions` false, and the UI then never offers the buttons.
- **Isolation is enforced.** `tests/architecture.rs` fails if an adapter imports another adapter,
  `tauri`, or the session manager/pump.

### Detection

`detect()` is a report, not a gate. It resolves the configured command (or the adapter's default)
without running anything, and `start()` is always allowed to try anyway: for a process-backed
adapter a missing binary becomes a retryable `error` event on the first turn, so the session row and
the transcript still exist.

- **Adapters with a default binary** (`claude`, `codex`, `opencode`, `pi`) resolve the configured
  override first, then `PATH`. `pi` also has to find its sidecar build (`sidecar/dist/main.js`) and
  Node; its detail explains which one is missing.
- **An adapter with no default binary reports `unavailable` until it is configured.** ACP is a
  protocol, not a product, so `AcpAdapter::detect` answers `installed: false` with the setting to
  change ("no ACP agent command is configured…"), and `start()` fails `AdapterError::Unavailable`
  rather than guessing a binary. `AcpAdapter::with_command` is the detection hint; a session's
  `config.command` is what actually launches. The App always shows the harness in the picker; the
  doctor explains the missing command instead of letting the first turn fail mysteriously.
- **Attach-mode OpenCode** detects the configured `baseUrl` rather than a binary: there is nothing
  to spawn when Grove is told to join a server that is already running.

### `resume_token`

The harness's own conversation id, mirrored into `sessions.resume_token` by the manager after every
command and at start. `session_resume` restarts the adapter with it in `StartContext.resume_token`.
Set `capabilities.resume = true` only if you consume that token. If you cannot, the manager replays
the transcript tail as `StartContext.prompt` instead — the plan and history survive either way.

A process-backed adapter usually learns the id from the harness's *first line*, after the manager's
post-command mirror has run. Return the `Arc<Mutex<Option<String>>>` you write from
`resume_token_slot()` and the session pump mirrors it into the row as soon as it changes — no second
command needed. In-process adapters can ignore the slot (the default is `None`).

The token is whatever the harness itself uses: Claude's `session_id`, Codex's thread id, the OpenCode
or ACP session id, and pi's session **file path**. Do not assume it is a UUID — persist the string you
were given and hand it back unchanged.

## The Claude Code adapter, as a worked example

`claude --print --output-format stream-json --verbose [--model <m>] [--resume <session_id>]`, prompt
on stdin, working directory = `StartContext.cwd`, context documents prepended to the prompt.

One process **per turn**, with `--resume` between turns. This is the documented headless form; the
control protocol (`interrupt`, `control_request`) is undocumented and version-dependent, so the
adapter does not depend on it. The trade-off is honest:

- cancel = kill the child process tree (`taskkill /PID <pid> /T /F` on Windows; the process group on
  Unix) — no acknowledgement to wait for in one-shot mode;
- `capabilities.permissions = false`: a one-shot `-p` run has **no interactive permission channel**.
  A tool call that would prompt is denied by the CLI, which arrives as an `error` event. Users grant
  access with `--permission-mode` / `--allowedTools` in `config.args`. When the CLI does send a
  `control_request`, the adapter logs it rather than pretending it can answer;
- `capabilities.diffs = false`: the CLI sends no patch for `Edit`/`Write`, and synthesizing one is a
  later ticket, not a quiet lie.

### Line mapping

| `stream-json` line | Grove events |
|---|---|
| `system` / `init` | session id captured (already reported in `SessionStarted`); model/capabilities logged |
| `system` / `api_retry` | `Error { retryable: true }` then `Status { busy }` |
| `system` / `permission_denied` | `Error { retryable: false }` explaining the missing permission channel |
| `assistant` → `text` blocks | `Delta { text }` then `Message { assistant, blocks }` |
| `assistant` → `thinking` block | `Message { assistant, [Thinking] }` (never streamed as a delta) |
| `assistant` → `tool_use` block | `ToolCall { call_id: id, name, title }`; title = `input.command`, else `input.file_path`, else `input.description`, else the tool name |
| `assistant` / `user` with `parent_tool_use_id` | skipped (subagent turns are not the main transcript) |
| `user` → `tool_result` block | `ToolResult { call_id: tool_use_id, status: is_error ? error : ok, summary }`, summary truncated |
| `stream_event` → `content_block_delta`/`text_delta` | `Delta { text }` (only when `--include-partial-messages` is configured) |
| `result` | `Usage { input_tokens, output_tokens }`, then `Status { idle }`; on `is_error`/`error_*`: `Error { retryable: false }` + `Status { error }` |
| `control_request` / `sdk_control_request` | logged and dropped (no answerable channel) |
| unknown `type`, malformed JSON, blank line | logged and skipped |
| child exits without `result` | `Error { retryable: true }` + `Status { exited }` |
| child exits after `result` | nothing more (the turn already terminated) |
| cancel | child tree killed, `Status { idle }` |

Every event that the adapter claims in `Capabilities` appears here. Adding a mapping means updating
both the code and this table.

## The ACP adapter, as a generic example

ACP is a protocol, not a product, so `acp.rs` knows no harness name: the command line comes entirely
from `config.command` and `config.args`. With no command configured there is nothing to detect and
nothing to launch (see §Detection above).

Launch sequence (ACP v1, `protocolVersion: 1` — v2 is a draft with renames, so the adapter never
negotiates up):

1. `initialize` with `protocolVersion: 1`, honest `clientCapabilities` and `clientInfo`. A version
the agent answers with that Grove does not speak ends the session as an `error`.
2. `session/new` with the session `cwd` and the configured `mcpServers`, or `session/resume` with the
   stored session id. `session/load` is never used: it **replays the whole conversation** as
   `session/update` notifications, which would duplicate Grove's append-only transcript (the fixture
   exits non-zero if a client uses it).
3. `session/prompt` per user message — one long-lived request per turn, answered with a `stopReason`;
   `session/cancel` (a notification) cuts it short.

`session/update` mapping, and nothing more (`Capabilities` matches this table exactly):

| `session/update` | Grove events |
|---|---|
| `agent_message_chunk` with text, same `messageId` | `Delta { text }` per chunk |
| `agent_message_chunk`, `messageId` changes or the turn ends | `Message { assistant, [Text] }` for the text streamed so far |
| `agent_thought_chunk` | `Message { assistant, [Thinking] }` |
| `tool_call` | `ToolCall { call_id: toolCallId, name, title }` |
| `tool_call_update` with `completed` / `failed` | `ToolResult { call_id, ok / error, summary }` |
| `tool_call_update` with `pending` / `in_progress` | logged (progress only) |
| `user_message_chunk` | ignored: it is the replay echo `session/load` would produce |
| `plan`, `usage_update`, `current_mode_update`, `config_option_update`, `session_info_update`, `available_commands_update`, `notice`, `compaction_*` | logged and ignored |
| unknown `sessionUpdate` variant, unknown notification | logged and ignored |
| unknown **request** (anything with an id) | answered with JSON-RPC `-32601`, so an agent never waits |
| `session/request_permission` | `PermissionRequest { request_id: <jsonrpc id>, title, detail }`, answered `{outcome:{outcome:"selected", optionId}}` or `{outcome:{outcome:"cancelled"}}` |
| `session/prompt` result | `stopReason` `end_turn`/`cancelled` → `Status { idle }`; `refusal`, `max_tokens`, `max_turn_requests` → `Error` + `Status { idle }` |
| transport close | `Status { exited }` |

Deliberate limits, reflected in `capabilities()`:

- `diffs: false` — ACP `diff` tool content is summarized, not turned into a patch yet.
- `usage: false` — `usage_update` reports context-window fill (`used`/`size`) and cost, not the
  input/output token split `Usage` carries, so nothing is invented.
- `images: false`, `mcp: false` — prompts are text, and although `mcpServers` is forwarded because the
  protocol requires it, Grove's MCP catalog is not wired into ACP sessions yet.
- No `authenticate` and no model selection: an agent advertising `authMethods` is reported rather than
  silently failing every turn, and `StartContext.model` is recorded on `SessionStarted` while the
  agent's own configuration decides what runs.
- No `session/close`: shutdown kills the agent's process tree and leaves the `exited` transition to
  the session manager.

Permissions are the place where "always answer" matters more than anything else: every
`session/request_permission` gets a response, including `cancelled` when the user cancels or when the
request arrives outside a turn — the protocol says a client **MUST** answer `cancelled` on
cancellation or the agent may hang. A cancelled turn keeps its deltas but never completes a
`Message`: fragments between `session/cancel` and the `cancelled` stop reason are accepted and
discarded rather than shown as a finished answer.

## The fixture pattern

Tests never touch a real harness or the network. Each adapter ships a scripted fake CLI:

```
fixtures/harnesses/claude_fake.mjs --mode <happy|tools|permission|error|crash|unknown|slow> [--delay-ms N]
```

It parses the same flags the adapter passes, reads the prompt from stdin, prints documented
newline-delimited JSON, and exits. The fake **fails loudly** when a required flag (`--print`,
`--output-format stream-json`, `--verbose`) is missing, so the flag list is tested rather than
assumed. `--resume` echoes the id it was given, which is what makes the resume-token roundtrip
observable.

The adapter is pointed at it entirely through config:

```rust
StartContext {
    config: serde_json::json!({
        "command": "node",
        "args": ["C:/repo/fixtures/harnesses/claude_fake.mjs", "--mode", "tools"],
    }),
    ..base_context()
}
```

Modes are chosen to exercise failure paths, not just the happy one: `permission` (a control request
the protocol cannot answer), `crash` (dies before `result`), `unknown` (garbage and future line
types interleaved with a valid answer), `slow` (still streaming when a cancel lands).

Guidelines for a new fake:

- one line per event, newline-delimited, exactly like the real protocol;
- cover: happy path, tool use, permission, error line, mid-stream crash, unknown lines;
- verify the argv the adapter claims to pass and exit non-zero when it is wrong;
- read stdin so the prompt path is real; echo part of it back so tests can prove it arrived;
- keep every delay configurable (`--delay-ms`) so the suite is fast and cancellable;
- never call the network.

### When the fixture is a server, not a CLI

Not every harness is a stdio child, so not every fixture is a fake CLI. Two shapes exist:

- **A transport host.** `fixtures/harnesses/pi_sidecar_fake.mjs` is a stand-in for the Node sidecar:
  it speaks **Grove Sidecar Protocol v1** (NDJSON JSON-RPC on stdio, see
  `docs/sidecar-protocol.md`) and replays already-normalized `HarnessEvent`s. That is why the Rust
  `pi` adapter has no idea pi exists — it only knows this protocol, and the fixture proves its side
  of the handshake, the forwarding and the failure modes.
- **A real server.** `fixtures/harnesses/opencode_fake.mjs` is an actual loopback HTTP + SSE server
  (`node opencode_fake.mjs serve --mode <mode>`), because OpenCode is reached over HTTP, not stdio.
  It binds `127.0.0.1`, prints the chosen port (and can write it to `--port-file`), keeps sessions in
  `--state-dir/sessions.json` so a restarted server can be resumed, and can append a
  `--heartbeat-file` so a test can prove the process died. Its `--log-file` records what the adapter
  really requested.

The fixture rules are the same: validate what the adapter sends, fail loudly on a wrong launch, make
failure modes selectable with `--mode`, and never reach the network. Conformance still applies — the
OpenCode target simply starts the server through `config.command`/`config.args` like every other
adapter, and the shared suite proves the same event ordering over a different transport.

## The conformance suite

`tests/adapters_conformance.rs` runs the same expectations against every adapter and its fixture:

| Check | What it proves |
|---|---|
| capabilities honesty | every claimed capability is observed in the stream; an adapter without permissions reports `Unsupported` from `respond_permission`; an adapter that claims `permissions` must opt in to the roundtrip below |
| ordering and termination | `session_started` first and once; `ready` before content; delta before its message; call before result; timestamps are RFC 3339 UTC; the turn ends `idle`, never `exited` |
| cancel | a slow turn is cut short, produces no error and no assistant message, returns to `idle`, and the session still works afterwards |
| resume token roundtrip | the token is captured, survives a restart with `resumed: true`, and the harness keeps the same conversation id |
| malformed input | unknown frames and junk input are skipped, the valid frames still land, and the session remains usable |
| permission roundtrip (opt-in) | the harness emits a `PermissionRequest`, the answer is delivered, and a matching `PermissionResolved` is recorded |

`Target::new(adapter, |scenario| context)` maps the scenario names `turn`, `slow`, `unknown` and
`permission` to whatever the fixture understands. The permission scenario is **opt-in**: a target
that claims `capabilities.permissions` must call `Target::with_permissions()` and script the
`permission` scenario (Codex's `approval` mode, ACP's `permission` mode). Without that opt-in the
suite fails, so a permission claim can never be a guess. The suite also fails if an adapter is
registered in `builtin_registry()` without a target, so a new harness cannot ship half-tested.

## Add a sixth harness: the six-step recipe

1. **Write the adapter module** `crates/grove-core/src/adapters/<harness>.rs`. Implement
   `HarnessAdapter` + `HarnessSession`, map the wire format to `EventKind`, capture the resume token,
   and read `config.command`/`config.args` with `support::command_from_config`. Use `claude.rs` as the
   shape: one actor task owning the process, a bounded event channel, an unbounded command channel,
   `cancel` handled inside the turn's `select!` so a kill cannot race a message. Take `find_on_path`,
   `render_prompt`, `truncate` and the kill helpers from `support` rather than copying them.
2. **Declare capabilities honestly**, plus a side-effect-free `detect()` that resolves the binary
   (honour a per-adapter command override). Leave `false` anything you do not actually emit. Add
   `display_name()` and — if the harness has one — `skills_dir()`; that is all the doctor and the
   Skills dropdown need to pick the adapter up.
3. **Write the fixture** `fixtures/harnesses/<harness>_fake.mjs` with the modes listed above, echoing
   the resume id and failing when Grove's own flags are wrong.
4. **Test each mode** in `crates/grove-core/tests/adapters_<harness>.rs` (happy plus one test per
   failure mode), and add a `Target` to `tests/adapters_conformance.rs` (plus `with_permissions()` if
   you claim the permission capability). Add the adapter to `tests/adapters_restart.rs` too: the same
   create → send → close → reopen → resume path runs against every registered harness. Green
   conformance is the definition of "contract-compliant".
5. **Register it in one line**: `crates/grove-core/src/adapters/registry.rs` — add the
   `Arc::new(<Harness>Adapter::default())` entry. That is the only shared file the feature touches
   besides `adapters/mod.rs` (`pub mod <harness>;`) if you re-export it. The picker, the doctor labels,
   the skills dropdown, `/spec_index`'s session scoping and the settings harness-path resolution all
   read the registry, so registration is what makes the harness real everywhere.
6. **Document and pin**: add the launch form and line-mapping table to this file, and if you surface
   a new DTO (or change one), add `contract/fixtures/<domain>.<case>.json`, a Rust assertion in
   `tests/contract_fixtures.rs`, and a zod parse in `packages/contract/test/`. Never invent a
   parallel shape for the session picker — it already consumes `HarnessInfo { id, capabilities }`.

No step touches `apps/desktop`, `packages/contract/src` (unless a DTO genuinely changes), the panel
registry, or the shell. The harness appears in the picker, streams into the same transcript, and
resumes from the same `session_resume` command as every other harness.

## Debugging an adapter

- `RUST_LOG=grove::adapters::<harness>=debug` — every skipped line is logged there, with the reason.
- The fixture is the oracle: if the fake and the real CLI disagree, record a line from the real CLI
  into the fake instead of guessing (see `protocols.md` in the research notes for primary sources).
- `session_get` returns the persisted transcript; if an event is missing from it, the adapter never
  emitted it (persist-then-broadcast means the log is never behind the UI).
- Cancel tests are timing-sensitive by nature: keep the fake's delay configurable and its streaming
  phase long enough to be interrupted on a loaded CI runner.
