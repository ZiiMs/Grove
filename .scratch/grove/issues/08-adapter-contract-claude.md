# 08: Adapter contract + Claude Code adapter + contributor guide

**What to build:** The adapter interface told by `architecture.md` becomes real code: one contract, a
registry, a mock, and a working Claude Code adapter that runs a real session over the CLI's
`stream-json` protocol. Plus the conformance test suite every later adapter must pass, and
`docs/adapters.md` — proof that a sixth harness can be added without touching UI code.

**Context pointers:** `architecture.md` §The adapter contract, §Normalized event model, §Extension seams.
Ticket 02 built the session manager and the mock adapter.

**Blocked by:** 02 (session core)

**Status:** ready-for-agent

- [ ] Contract and registry finalized per architecture; `builtin_registry()` returns mock + claude;
      a test asserts no adapter imports another adapter, `tauri`, or UI code
- [ ] Claude adapter spawns `claude --print --output-format stream-json --verbose [--resume <id>]
      [--model <m>]` in the session cwd, feeds the prompt (and context docs) and maps stream-json lines
      to normalized events: assistant text, `tool_use` → `tool_call`, `tool_result`, result + usage,
      errors; it captures the session id as the resume token
- [ ] Cancel kills the child process tree; an unexpected exit mid-stream yields `status exited` plus a
      retryable `error`
- [ ] Unknown or malformed lines are logged and skipped, never fatal
- [ ] Permission requests are emitted and answerable when the protocol allows; otherwise
      `capabilities.permissions` is honestly `false`
- [ ] `fixtures/harnesses/claude_fake.mjs` covers happy path, tool use, permission, error, crash; one
      adapter test per mode
- [ ] A shared conformance suite (capabilities honesty, event ordering, cancel, resume token roundtrip,
      malformed-line tolerance) runs against every registered adapter and its fixture
- [ ] `docs/adapters.md` documents the trait, the event mapping, the fixture pattern, and the six-step
      recipe for adding a harness
- [ ] Contract fixtures for `Capabilities` and `Detection`
