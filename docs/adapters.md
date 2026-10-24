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

Rules that keep adapters isolated and Grove trustworthy:

- **Forward compatible by default.** An unknown line, event type, or JSON-RPC method is logged and
  dropped. Protocol drift is a log line, not a dead session.
- **No panics on the wire.** Malformed input becomes a logged skip, an `EventKind::Error`, or an
  `AdapterError` — never a panic.
- **Capability honesty.** A flag is `true` only if the adapter really emits that event kind for the
  launch shape Grove uses. The UI degrades from `Capabilities`, never from the harness name.
- **Explicit command override.** Read `config.command` and `config.args`. A user points Grove at a
  wrapper; tests point it at a fixture.
- **Detection never blocks starting.** `detect()` is side-effect free (resolve on `PATH`, no probes)
  and its result does not gate `start()`. A missing binary becomes an error *event* on the first
  turn, so the session row and transcript still exist; the doctor explains it in Settings.
- **One writer per session.** The session manager serializes `send`/`cancel`/`respond_permission`
  behind a per-session mutex. An adapter's actor must still never run two harness processes against
  one conversation: queue prompts and run them in order.
- **Isolation is enforced.** `tests/architecture.rs` fails if an adapter imports another adapter,
  `tauri`, or the session manager/pump.

### `resume_token`

The harness's own conversation id, mirrored into `sessions.resume_token` by the manager after every
command and at start. `session_resume` restarts the adapter with it in `StartContext.resume_token`.
Set `capabilities.resume = true` only if you consume that token. If you cannot, the manager replays
the transcript tail as `StartContext.prompt` instead — the plan and history survive either way.

A process-backed adapter usually learns the id from the harness's *first line*, after the manager's
post-command mirror has run. Return the `Arc<Mutex<Option<String>>>` you write from
`resume_token_slot()` and the session pump mirrors it into the row as soon as it changes — no second
command needed. In-process adapters can ignore the slot (the default is `None`).

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

## The conformance suite

`tests/adapters_conformance.rs` runs the same expectations against every adapter and its fixture:

| Check | What it proves |
|---|---|
| capabilities honesty | every claimed capability is observed in the stream; an adapter without permissions reports `Unsupported`; an unverified permission claim fails the suite |
| ordering and termination | `session_started` first and once; `ready` before content; delta before its message; call before result; timestamps are RFC 3339 UTC; the turn ends `idle`, never `exited` |
| cancel | a slow turn is cut short, produces no error and no assistant message, returns to `idle`, and the session still works afterwards |
| resume token roundtrip | the token is captured, survives a restart with `resumed: true`, and the harness keeps the same conversation id |
| malformed input | unknown frames and junk input are skipped, the valid frames still land, and the session remains usable |

`Target::new(adapter, |scenario| context)` maps the scenario names `turn`, `slow`, and `unknown` to
whatever the fixture understands. The suite also fails if an adapter is registered in
`builtin_registry()` without a target, so a new harness cannot ship half-tested.

## Add a sixth harness: the six-step recipe

1. **Write the adapter module** `crates/grove-core/src/adapters/<harness>.rs`. Implement
   `HarnessAdapter` + `HarnessSession`, map the wire format to `EventKind`, capture the resume token,
   and read `config.command`/`config.args`. Use `claude.rs` as the shape: one actor task owning the
   process, a bounded event channel, an unbounded command channel, `cancel` handled inside the turn's
   `select!` so a kill cannot race a message.
2. **Declare capabilities honestly**, plus a side-effect-free `detect()` that resolves the binary
   (honour a per-adapter command override). Leave `false` anything you do not actually emit.
3. **Write the fixture** `fixtures/harnesses/<harness>_fake.mjs` with the modes listed above, echoing
   the resume id and failing when Grove's own flags are wrong.
4. **Test each mode** in `crates/grove-core/tests/adapters_<harness>.rs` (happy plus one test per
   failure mode), and add a `Target` to `tests/adapters_conformance.rs`. Green conformance is the
   definition of "contract-compliant".
5. **Register it in one line**: `crates/grove-core/src/adapters/registry.rs` — add the
   `Arc::new(<Harness>Adapter::default())` entry. That is the only shared file the feature touches
   besides `adapters/mod.rs` (`pub mod <harness>;`) if you re-export it.
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
