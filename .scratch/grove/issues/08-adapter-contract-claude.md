# 08: Adapter contract + Claude Code adapter + contributor guide

**What to build:** The adapter interface told by `architecture.md` becomes real code: one contract, a
registry, a mock, and a working Claude Code adapter that runs a real session over the CLI's
`stream-json` protocol. Plus the conformance test suite every later adapter must pass, and
`docs/adapters.md` — proof that a sixth harness can be added without touching UI code.

**Context pointers:** `architecture.md` §The adapter contract, §Normalized event model, §Extension seams.
Ticket 02 built the session manager and the mock adapter.

**Blocked by:** 02 (session core)

**Status:** done

- [x] Contract and registry finalized per architecture; `builtin_registry()` returns mock + claude;
      a test asserts no adapter imports another adapter, `tauri`, or UI code
- [x] Claude adapter spawns `claude --print --output-format stream-json --verbose [--resume <id>]
      [--model <m>]` in the session cwd, feeds the prompt (and context docs) and maps stream-json lines
      to normalized events: assistant text, `tool_use` → `tool_call`, `tool_result`, result + usage,
      errors; it captures the session id as the resume token
- [x] Cancel kills the child process tree; an unexpected exit mid-stream yields `status exited` plus a
      retryable `error`
- [x] Unknown or malformed lines are logged and skipped, never fatal
- [x] Permission requests are emitted and answerable when the protocol allows; otherwise
      `capabilities.permissions` is honestly `false`
- [x] `fixtures/harnesses/claude_fake.mjs` covers happy path, tool use, permission, error, crash; one
      adapter test per mode
- [x] A shared conformance suite (capabilities honesty, event ordering, cancel, resume token roundtrip,
      malformed-line tolerance) runs against every registered adapter and its fixture
- [x] `docs/adapters.md` documents the trait, the event mapping, the fixture pattern, and the six-step
      recipe for adding a harness
- [x] Contract fixtures for `Capabilities` and `Detection`

## Resolution

Merged as `feat(adapters): add the Claude Code adapter, conformance suite, and contributor guide`.

Decisions worth knowing:

- **One process per turn** (`claude --print --output-format stream-json --verbose`, `--resume <id>` for
  follow-ups) rather than long-lived stream-input mode: it is the documented headless form, cancellation is
  a process-tree kill, and it does not depend on the undocumented control-request channel.
- `capabilities.permissions = false` and `diffs = false` are deliberate honest answers: a one-shot `-p`
  run has no answerable permission channel, and the CLI emits no patch (synthesising diffs was out of
  scope). The UI degrades from capabilities, never from the harness name.
- **Contract addition:** `HarnessSession::resume_token_slot()` (defaulted) plus pump wiring, because a
  process adapter learns its session id from the first stream line — after the manager's post-command
  mirror — so the token was otherwise never persisted. `architecture.md` §Session runtime now records it.
- The conformance suite cross-checks the registry against its own target list in both directions, so a new
  adapter cannot be registered without conformance coverage (tickets 10–13 rely on this).
- `docs/adapters.md` is the sixth-harness recipe: trait, event mapping, fixture pattern, and "touch no UI".

Not verified: the real `claude` CLI was never invoked (by design — no install or network in tests), and the
Unix process-group kill path is `cfg`-gated and therefore compiled only on non-Windows CI.
