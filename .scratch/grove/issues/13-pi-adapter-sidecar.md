# 13: `pi` adapter + Node sidecar (Grove Sidecar Protocol v1)

**What to build:** A `pi` session in Grove, run through a Node sidecar that fronts pi's RPC mode. The
sidecar is a documented, swappable process boundary: any program speaking the protocol can drive a pi
session, and the Rust side never depends on a JS package layout.

**Context pointers:** `architecture.md` §Sidecar protocol, §The adapter contract. Ticket 08 owns the
conformance suite and fixtures. **Protocol reference:** `C:/Users/zyko8/.grove-notes/protocols.md` §5 (pi)
— RPC mode is `pi --mode rpc`, JSONL split on LF only (never Node `readline`, which also splits on
U+2028/29), the idle signal is `agent_settled` (not `agent_end`), and the resume token is the
`sessionPath` from `get_session_stats`.

**Blocked by:** 08 (adapter contract + Claude Code adapter)

**Status:** ready-for-agent

- [ ] `docs/sidecar-protocol.md` specifies Grove Sidecar Protocol v1 exactly as architecture.md describes:
      NDJSON JSON-RPC 2.0, `initialize`, `session.start/send/permission/cancel/stop`, and a
      `session.event` notification carrying a normalized `HarnessEvent`
- [ ] The sidecar implements that protocol and drives `pi --mode rpc` (configurable command/args), mapping
      pi's `agent_start` / `message_update{assistantMessageEvent.type:"text_delta"}` / `tool_execution_*` /
      `agent_settled` to normalized events. When the binary is absent it reports `unavailable` with a
      reason instead of failing the handshake; an optional SDK path may be documented as future work, not
      built
- [ ] The Rust `pi` adapter spawns the sidecar (config command, default `node sidecar/dist/main.js`),
      performs the handshake, forwards events, and maps handshake failure to `unavailable`
- [ ] Malformed sidecar output is logged and dropped; a dead sidecar yields `status exited` plus a
      retryable `error`
- [ ] The sidecar child is killed on shutdown (no orphans), verified by test; a resume token (pi's session
      file path) roundtrips through `session.start`
- [ ] Sidecar tests drive the RPC layer against a fake pi RPC child; Rust tests use
      `fixtures/harnesses/pi_sidecar_fake.mjs`; the shared conformance suite passes
- [ ] The sidecar builds from the workspace (`npm run build`) and its dist is what the adapter launches
- [ ] Registered in `builtin_registry()`; doctor reports Node presence and whether the sidecar build exists
