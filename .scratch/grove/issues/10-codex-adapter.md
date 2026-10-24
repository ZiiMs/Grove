# 10: Codex adapter — app-server JSON-RPC (v2 thread/turn API)

**What to build:** A real Codex session in Grove: Grove starts `codex app-server`, starts a thread, sends
turns, and shows messages, command executions, aggregated diffs, token usage, and approval prompts in the
same UI as every other harness. The thread survives a restart.

**Context pointers:** `architecture.md` §The adapter contract, §Normalized event model. Ticket 08 owns the
contract, the conformance suite, and the fixture pattern — follow them. **Protocol reference:**
`C:/Users/zyko8/.grove-notes/protocols.md` §2 (Codex). It documents the *current* v2 thread/turn API and is
authoritative over older v1 method names (`newConversation`, `sendUserTurn` and `codex/event/*` no longer
exist).

**Blocked by:** 08 (adapter contract + Claude Code adapter)

**Status:** ready-for-agent

- [ ] Spawns `codex app-server` (command/args overridable by config) and performs the handshake:
      `initialize` → the `initialized` notification → `thread/start` → `turn/start` with
      `input: [{type: "text", text, text_elements: []}]`; no `protocolVersion` field is expected
- [ ] Notifications map to normalized events: `item/agentMessage/delta` (plus `item/started` /
      `item/completed` for the final message), `commandExecution` items → `tool_call`/`tool_result` with
      status, aggregated output and exit code, `turn/diff/updated` → `diff` (aggregated unified diff per
      turn), `thread/tokenUsage/updated` → `usage`, `turn/completed` → `status idle`
- [ ] Approval requests become `permission_request`, are answerable via `respond_permission`, and resolve
      as `permission_resolved`; if the protocol offers no approval channel, `capabilities.permissions` is
      honestly `false`
- [ ] Cancel sends `turn/interrupt {threadId, turnId}`; a structured turn error is reported without
      ending the session
- [ ] The thread id is stored as the resume token and reused with `thread/resume {threadId}` so a restart
      continues the same conversation (no replay is assumed)
- [ ] Unknown methods and notifications are logged and ignored; protocol drift never crashes a session
- [ ] `fixtures/harnesses/codex_fake.mjs` covers happy path, approval, diff, error, crash; the shared
      conformance suite passes
- [ ] Capabilities accurately report diffs, permissions, and resume
- [ ] Registered in `builtin_registry()`; doctor recognises the `codex` binary
