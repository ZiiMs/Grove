# 12: OpenCode adapter — local HTTP server + SSE event stream

**What to build:** A user runs an OpenCode session in Grove: Grove either attaches to an already-running
local server or starts `opencode serve` itself, then shows messages, tool calls, diffs, errors, and
cancellation. Fully local — no external network.

**Context pointers:** `architecture.md` §The adapter contract, §Normalized event model. Ticket 08 owns the
conformance suite and fixtures. **Protocol reference:** `C:/Users/zyko8/.grove-notes/protocols.md` §4
(OpenCode) — readiness is `GET /global/health`, not a guessed port banner.

**Blocked by:** 08 (adapter contract + Claude Code adapter)

**Status:** done

- [x] Config accepts a server URL (attach) or spawns `opencode serve --port <free>`; readiness is awaited
      by polling `GET /global/health` with a timeout, and captured stderr appears in the failure reason
- [x] A session is created over HTTP, user turns are sent over HTTP, and `/event` SSE is consumed and
      mapped to normalized events, including `diff` events from edit tools
- [x] Cancel aborts the in-flight turn; the spawned server child is killed on shutdown (no orphans),
      verified by test
- [x] SSE disconnect reconnects with backoff while the session is alive; a permanent failure surfaces as
      `error` plus `status exited`
- [x] The OpenCode session id is the resume token, reused on restart; an attach-mode URL is remembered for
      the session
- [x] `fixtures/harnesses/opencode_fake.mjs` serves REST + SSE and covers happy path, diff, disconnect and
      reconnect, cancel; the shared conformance suite passes
- [x] Ports are chosen dynamically; no fixed port is assumed and nothing binds outside loopback
- [x] Registered in `builtin_registry()`; doctor reports `opencode` presence and any configured URL

## Resolution

Merged as `feat(adapters): add the OpenCode adapter over loopback HTTP and SSE`.

Decisions worth knowing:

- Both modes are real: attach to a running server (the URL is remembered in the session's adapter config) or
  spawn `opencode serve --port <free>` on loopback, with readiness proven by polling `/global/health`
  (stderr tail is surfaced on failure) rather than guessing a port banner.
- The SSE reader reconnects with capped backoff while the session is alive; a permanent failure becomes a
  non-retryable `error` plus `status exited`, and the child is reaped — verified by a heartbeat file and a
  refused TCP connect.
- `capabilities.permissions = false`: the OpenCode permission response enum is unconfirmed, so a permission
  prompt is surfaced as an error telling the user to approve in the OpenCode TUI rather than faking a claim.
- Only the documented v1 surface is used (`/session`, `/event`, `/global/health`); v2 endpoints are ignored
  and documented as such.
- The adapter's HTTP client sets `no_proxy()`, so a proxy environment variable cannot reroute loopback
  traffic.
- Spawn-mode resume against a dead server 404s by design: the adapter starts a new conversation and reports
  the lost id; attach mode (or a server whose state outlives the client) truly continues.

Not verified: no real `opencode` binary or real `/event` stream was exercised (fixtures only); the known
`prompt_async` never-starts bug in v1.14.44 has no turn-start watchdog; and a close arriving during server
startup is handled only after bring-up completes.
