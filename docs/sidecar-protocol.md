# Grove Sidecar Protocol v1

The sidecar is a **documented, swappable process boundary**. A sidecar is any program that speaks
this protocol on stdio: it owns a harness's wire format and rewrites it as normalized Grove events,
so the Rust core never depends on a JavaScript package layout, an SDK, or a harness's private
protocol. The first implementation drives `pi` (`sidecar/src/pi_host.ts`); the Rust `pi` adapter is
its host (`crates/grove-core/src/adapters/pi.rs`).

Why a process rather than an FFI/SDK dependency: the harness's protocol is then replaceable without
touching Rust, the adapter is testable against a scripted fixture
(`fixtures/harnesses/pi_sidecar_fake.mjs`), and a session can never take the core down with it. An
in-process SDK path for `pi` is **future work, not built**: it would sit *behind* this protocol and
change nothing above it.

## Transport

- **Newline-delimited JSON (NDJSON), JSON-RPC 2.0.** Exactly one JSON object per line, `\n`
  terminated. Every message carries `"jsonrpc":"2.0"`.
- **Framing is LF-only.** A record ends at `\n`; a preceding `\r` is tolerated and dropped. Blank
  lines are ignored. **Do not** use a reader that also splits on `U+2028`/`U+2029` (Node's `readline`
  does, and both characters are legal unescaped inside JSON strings) — that tears a valid record in
  half. Both sides split on LF only: `createLineSplitter` (`sidecar/src/ndjson.ts`) and
  `BufReader::lines` in `crates/grove-core/src/adapters/pi.rs`, which also drops a trailing CR.
- **stdout carries protocol only.** Diagnostics go to stderr; the parent parses stdout.
- **Backpressure.** A reader that stops reading stalls the harness, so stdout is drained
  continuously, and a writer with something large to send honours the stream's backpressure. In v1 no
  message is large enough to need queuing (commands are small, events are bounded per frame).
- **No network.** A sidecar and the harness behind it inherit the user's auth; nothing in a session
  path performs a network call.
- **Process lifetime.** The sidecar exits when stdin closes (the parent died), and releases every
  harness child first. The host kills the whole process tree on shutdown, so nothing is orphaned.

## Messages

```jsonc
// request — the parent asks, the sidecar answers
{"jsonrpc":"2.0","id":1,"method":"session.send","params":{"sessionId":"s1","text":"hi"}}

// response — same id, exactly one of result/error
{"jsonrpc":"2.0","id":1,"result":{}}
{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"pi is not installed","data":{"reason":"…"}}}

// notification — no id, no response
{"jsonrpc":"2.0","method":"session.event","params":{"sessionId":"s1","event":{…}}}
```

`id` is a string or a number; it is echoed verbatim. The parent sends requests only (v1 defines no
parent→sidecar notification, and a notification without an `id` is accepted silently). Unknown
methods answer `-32601`; unknown notifications are logged and dropped.

### Error codes

| Code | Meaning |
|---|---|
| `-32700` | Parse error: the line was not JSON |
| `-32600` | Invalid request: not a JSON-RPC 2.0 request |
| `-32601` | Method not found |
| `-32602` | Invalid params (missing/blank `sessionId`, `text`, `decision`, …) |
| `-32603` | Internal error: the sidecar failed in a way it cannot classify |
| `-32000` | **Unavailable**: the harness cannot be launched or is not installed. Always carries a human-readable `message`, usually also `data.reason` |
| `-32001` | **Unsupported**: the harness has no such operation (for example `session.permission` on `pi`) |
| `-32002` | **Not found**: no such `sessionId` |

The parent maps `-32000`/`-32001` to its own `unavailable`/`unsupported` error, so a user sees "install
pi" in Settings instead of a failed session.

## Methods

### `initialize`

```jsonc
→ {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}
← {"jsonrpc":"2.0","id":1,"result":{
     "protocolVersion":1,
     "version":"0.1.0",
     "harnesses":["pi"],
     "pi":{"available":true,"command":"C:/tools/pi.cmd"}
   }}
```

`version` is the sidecar's own build version. `harnesses` lists the harness ids the sidecar can drive.
`pi.available:false` carries a `reason` — this is the *default* resolution (environment/argv), used to
explain the situation before a session exists; `session.start` is authoritative because it receives
the per-session config.

A host that speaks another `protocolVersion` must refuse the session (`unavailable`) rather than
guess: v1 is versioned by that integer.

### `session.start`

```jsonc
→ {"jsonrpc":"2.0","id":2,"method":"session.start","params":{
     "sessionId":"s1",
     "cwd":"C:/work/grove",
     "model":"pi-sonnet",
     "prompt":"review the diff",
     "resumeToken":"C:/Users/me/.pi/sessions/abc.jsonl",
     "config":{"pi":{"command":"pi","args":["--mode","rpc"],"sessionDir":"C:/pi/sessions"}},
     "context":[{"id":"grove-architecture","title":"Grove — architecture","body":"# …"}]
   }}
← {"jsonrpc":"2.0","id":2,"result":{"resumeToken":"C:/Users/me/.pi/sessions/abc.jsonl"}}
```

| Field | Meaning |
|---|---|
| `sessionId` | Grove's id for this session. Required, non-empty; every event is tagged with it |
| `cwd` | Working directory for the harness. Required |
| `model` | Optional model override |
| `prompt` | Optional first user prompt. `context` and `prompt` are queued as the first turn |
| `resumeToken` | Reopen the harness's previous conversation. `pi`: the session **file path** |
| `config` | Adapter-specific config from Grove settings. `config.pi` is the harness launch config (`command`, `args`, `sessionDir`). Keys a host does not know are ignored, so the same object also carries the launcher settings (`command`, `args`, `sidecar`) that only the parent reads |
| `context` | Pinned specs and the plan, already rendered to markdown (`{id, title, body}`) |

The result carries the token the parent should mirror into `sessions.resume_token` — `pi`'s session
file path, learned from `get_session_stats`. A session without a token is not resumable; a session
that *resumes* reports the same token back.

Failures are `-32000` with the reason ("pi is not installed or not on PATH (looked for `pi`)"), and the
parent turns that into an `unavailable` error instead of starting a session it cannot drive.

### `session.send`

```jsonc
→ {"jsonrpc":"2.0","id":3,"method":"session.send","params":{"sessionId":"s1","text":"run the tests"}}
← {"jsonrpc":"2.0","id":3,"result":{}}
```

The result only says the prompt was *accepted*: the turn's progress arrives as `session.event`
notifications and ends with `status: idle`. A host queues prompts that arrive while a turn is running
and runs them in order — one harness conversation never has two writers. An unknown `sessionId` is
`-32002`.

### `session.permission`

```jsonc
→ {"jsonrpc":"2.0","id":4,"method":"session.permission",
   "params":{"sessionId":"s1","requestId":"ui-1","decision":"approve"}}
← {"jsonrpc":"2.0","id":4,"result":{}}
```

`decision` is `"approve"` or `"deny"`. A harness without an approval gate answers `-32001` (as `pi` does
(see [Permissions](#permissions)). The parent reports `unsupported` and the UI does not offer the
control when `Capabilities.permissions` is false.

### `session.cancel`

```jsonc
→ {"jsonrpc":"2.0","id":5,"method":"session.cancel","params":{"sessionId":"s1"}}
← {"jsonrpc":"2.0","id":5,"result":{}}
```

Cancel is a **request**, not a kill: the host asks the harness to stop, waits for its acknowledgement
(`pi`: the `abort` response plus `agent_settled`), then publishes `status: idle`. Frames that belong
to the cancelled turn are dropped, so a cancelled turn never produces an assistant message. A cancel
with nothing running is a no-op that still confirms `idle`. Cancelling an unknown session is a no-op.

### `session.stop`

Halt the harness and release its process. The parent sends it before killing anything; the sidecar
answers, kills its harness child, and stops publishing events for that session. Notifications already
written may still arrive — the parent ignores events for a session it has torn down.

## `session.event` notification

The only notification in v1. `event` is an **already-normalized** `HarnessEvent`, exactly the shape
`crates/grove-core/src/sessions/event.rs` persists and the webview renders. The parent re-validates it
with serde and drops what it cannot deserialize: a shape drift is a dropped event, never a broken
session.

```jsonc
{"jsonrpc":"2.0","method":"session.event","params":{"sessionId":"s1","event":{
  "at":"2025-01-01T00:00:00.000Z",   // RFC 3339, milliseconds, UTC
  "kind":"delta",                     // the discriminant; the payload is flattened beside it
  "text":"Hel"
}}}
```

| `kind` | Payload |
|---|---|
| `session_started` | `harness`, `model`, `resumed` — exactly once, and the first event of a session |
| `delta` | `text` — streamed assistant text |
| `message` | `role` (`user`/`assistant`/`system`), `blocks` (`[{type:"text"|"thinking"|"code", …}]`) |
| `tool_call` | `call_id`, `name`, `title` |
| `tool_result` | `call_id`, `status` (`ok`/`error`/`cancelled`), `summary` |
| `diff` | `path`, `patch`, `added`, `removed` |
| `permission_request` | `request_id`, `title`, `detail` |
| `permission_resolved` | `request_id`, `decision` |
| `usage` | `input_tokens`, `output_tokens` |
| `status` | `state` (`starting`/`ready`/`busy`/`idle`/`exited`/`error`) |
| `error` | `message`, `retryable` |

Ordering contract the host must keep (it is what the conformance suite checks): `session_started`
first and once; `ready` before any content; a `delta` before the `message` that completes it; a
`tool_call` before its `tool_result`; a turn ends `idle`, never `exited`. `exited` is reserved for a
harness that is gone, and comes with a retryable `error`.

## Lifecycle

```
parent                                sidecar                          harness
  │ initialize ───────────────────────► │                                │
  │ ◄────────────────── {protocolVersion, harnesses, pi}                │
  │ session.start ─────────────────────► │ spawn ────────────────────────►│
  │                                      │ ◄── handshake/state ───────────│
  │ ◄── session.event session_started …  │                                │
  │ ◄────────────── {resumeToken}        │                                │
  │ session.send ──────────────────────► │ prompt ───────────────────────►│
  │ ◄── session.event delta/message/…    │ ◄── deltas, tools, usage ──────│
  │ ◄── session.event status idle        │ ◄── settled ───────────────────│
  │ session.cancel ────────────────────► │ abort ────────────────────────►│
  │ ◄── session.event status idle        │ ◄── settled{aborted} ──────────│
  │ session.stop ──────────────────────► │ kill harness, exit             │
  │ stdin closes                         │ exit (no orphans)              │
```

A harness that dies mid-turn is `error{retryable:true}` followed by `status{exited}`; the parent keeps
the session row and its transcript, and the user resumes from the stored token.

## The `pi` implementation

`sidecar/src/pi_host.ts` launches pi in RPC mode and rewrites its records:

```bash
pi --mode rpc [--model <model>] [--session-dir <dir>]   # JSONL on stdin/stdout, logs on stderr
```

The launch is `config.pi.command` (default: `$GROVE_PI_COMMAND`, else `pi`) with
`config.pi.args` (default: `["--mode","rpc"]`) — plus `--session-dir`/`--model` when configured and not
already present. The sidecar process itself is invoked as

```bash
node sidecar/dist/main.js [--pi-command <cmd>] [--pi-arg <arg>]... [--quiet]
```

so a single `pi` can be pinned for every session on a machine without touching Grove's settings. The command is resolved on `PATH` (plus `PATHEXT` on Windows) before spawning, so a
missing binary is a `-32000` with a reason rather than a spawn error.

### pi → Grove mapping

| pi record | Grove event |
|---|---|
| `agent_start` | `status{busy}` (if not already busy) |
| `message_update` `assistantMessageEvent.type:"text_delta"` | `delta{delta}` |
| `thinking_delta` / `thinking_end` | folded into the assistant message's `thinking` block |
| `message_end` (assistant) | `message{assistant, [thinking?, text]}` — authoritative; buffered deltas are discarded |
| `tool_execution_start` | `tool_call{call_id:toolCallId, name:toolName, title:args.command ?? args.path ?? toolName}` |
| `tool_execution_end` | `tool_result{call_id, status: isError ? error : ok, summary: result content}` |
| `message_update.usage` / `turn_end.message.usage` | `usage{input, output}` — only when it changes, and never for pi's all-zero placeholder |
| `agent_settled` | `status{idle}` — **the idle signal** (`aborted:true` still means idle) |
| `auto_retry_start` | `error{retryable:true}` (+ `status{busy}`) |
| `agent_end`, `turn_start`, `message_start`, `compaction_*`, `auto_retry_end` | logged only — `agent_end` is **not** the end of work |
| `tool_execution_update`, `bash_execution_update` | logged only (progress folds into the final result) |
| `extension_ui_request`, `extension_error` | logged and dropped (see below) |
| unknown `type`, malformed JSON | logged and dropped |
| process exits | `error{retryable:true}` + `status{exited}` |

### Resume

The resume token is pi's **session file path** (`data.sessionFile` from `get_session_stats`), not the
session id. Resuming sends `switch_session {sessionPath}`; the sidecar then reports the token in
`session.start`'s result so the parent can mirror it. `--no-session` disables persistence and therefore
resume — a deployment that sets it should expect no token.

### Permissions

pi has **no tool-approval gate** in its wire protocol: tools run without asking. The only
request/response surface is the *extension UI* sub-protocol (`extension_ui_request` with
`confirm`/`select`/`input`/`editor`), which exists for extensions, not for Grove. v1 therefore reports
`permissions:false` for pi, answers `session.permission` with `-32001`, and logs extension UI traffic
instead of answering it with a guessed default. Mapping extension `confirm` onto
`permission_request`/`permission_resolved` is possible later, but it would be an honest capability
claim only once the conformance suite has a permission scenario.

Diffs are the same story: pi sends no patch, so `diffs:false` rather than a synthesized guess.

## Implementing a host

A sidecar is a program; it needs no Grove code. To add one:

1. Read stdin with an LF-only splitter, decode JSON-RPC 2.0, and dispatch the methods above.
2. Publish `session.event` notifications with the normalized `HarnessEvent` shape, keeping the
   ordering contract.
3. Answer `-32000` with a reason when the harness cannot be launched; never fail the handshake for a
   missing binary.
4. Release every harness child when stdin closes, on `session.stop`, and on a signal handler.

The Rust side needs no changes to host another harness behind this protocol: it is the same adapter
with a different `config.command`/`config.args` (or a different `config.sidecar`), and the events are
already normalized.

## Rust host configuration (`config` in `StartContext`)

| Key | Default | Meaning |
|---|---|---|
| `command` | `node` | The sidecar launcher |
| `args` | `[<workspace>/sidecar/dist/main.js]` | Sidecar argv. `npm run build` writes `sidecar/dist/main.js`; discovery walks up from the executable, the crate root and the working directory |
| `sidecar` | — | Explicit path to the sidecar entry (`$GROVE_SIDECAR` is the environment equivalent) |
| `pi` | — | Forwarded unchanged to `session.start` as `config.pi` |

`session.start` also forwards `cwd`, `model`, `prompt`, `resumeToken` and `context` from
`StartContext`. A handshake failure — the sidecar cannot be launched, dies, times out, speaks another
protocol version, or answers `-32000` — is an `unavailable` adapter error with the reason the sidecar
gave.
