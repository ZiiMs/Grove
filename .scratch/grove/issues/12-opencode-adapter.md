# 12: OpenCode adapter — local HTTP server + SSE event stream

**What to build:** A user runs an OpenCode session in Grove: Grove either attaches to an already-running
local server or starts `opencode serve` itself, then shows messages, tool calls, diffs, errors, and
cancellation. Fully local — no external network.

**Context pointers:** `architecture.md` §The adapter contract, §Normalized event model. Ticket 08 owns the
conformance suite and fixtures.

**Blocked by:** 08 (adapter contract + Claude Code adapter)

**Status:** ready-for-agent

- [ ] Config accepts a server URL (attach) or spawns `opencode serve --port <free>`; readiness is awaited
      with a timeout and captured stderr appears in the failure reason
- [ ] A session is created over HTTP, user turns are sent over HTTP, and `/event` SSE is consumed and
      mapped to normalized events, including `diff` events from edit tools
- [ ] Cancel aborts the in-flight turn; the spawned server child is killed on shutdown (no orphans),
      verified by test
- [ ] SSE disconnect reconnects with backoff while the session is alive; a permanent failure surfaces as
      `error` plus `status exited`
- [ ] The OpenCode session id is the resume token, reused on restart; an attach-mode URL is remembered for
      the session
- [ ] `fixtures/harnesses/opencode_fake.mjs` serves REST + SSE and covers happy path, diff, disconnect and
      reconnect, cancel; the shared conformance suite passes
- [ ] Ports are chosen dynamically; no fixed port is assumed and nothing binds outside loopback
- [ ] Registered in `builtin_registry()`; doctor reports `opencode` presence and any configured URL
