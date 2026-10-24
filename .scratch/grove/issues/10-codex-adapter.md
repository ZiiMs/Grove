# 10: Codex adapter — app-server JSON-RPC

**What to build:** A real Codex session in Grove: Grove starts `codex app-server`, starts a conversation,
sends turns, and shows messages, command executions, file diffs, token usage, and approval prompts in the
same UI as every other harness. The thread survives a restart.

**Context pointers:** `architecture.md` §The adapter contract, §Normalized event model. Ticket 08 owns the
contract, the conformance suite, and the fixture pattern — follow them.

**Blocked by:** 08 (adapter contract + Claude Code adapter)

**Status:** ready-for-agent

- [ ] Spawns `codex app-server` (command/args overridable by config), performs the initialize handshake,
      starts a conversation with cwd, model, and approval policy, and sends user turns
- [ ] Notifications map to normalized events: agent message deltas/final, command execution →
      `tool_call`/`tool_result` with an output summary, file changes → `diff`, token usage, errors
- [ ] Approval requests become `permission_request`, are answerable via `respond_permission`, and resolve
      as `permission_resolved`
- [ ] Cancel interrupts the active turn; a structured turn error is reported without ending the session
- [ ] The thread id is stored as the resume token and reused so a restart continues the same conversation
- [ ] Unknown methods and notifications are logged and ignored; protocol drift never crashes a session
- [ ] `fixtures/harnesses/codex_fake.mjs` covers happy path, approval, diff, error, crash; the shared
      conformance suite passes
- [ ] Capabilities accurately report diffs, permissions, and resume
- [ ] Registered in `builtin_registry()`; doctor recognises the `codex` binary
