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

**Status:** done

- [x] Spawns `codex app-server` (command/args overridable by config) and performs the handshake:
      `initialize` → the `initialized` notification → `thread/start` → `turn/start` with
      `input: [{type: "text", text, text_elements: []}]`; no `protocolVersion` field is expected
- [x] Notifications map to normalized events: `item/agentMessage/delta` (plus `item/started` /
      `item/completed` for the final message), `commandExecution` items → `tool_call`/`tool_result` with
      status, aggregated output and exit code, `turn/diff/updated` → `diff` (aggregated unified diff per
      turn), `thread/tokenUsage/updated` → `usage`, `turn/completed` → `status idle`
- [x] Approval requests become `permission_request`, are answerable via `respond_permission`, and resolve
      as `permission_resolved`; if the protocol offers no approval channel, `capabilities.permissions` is
      honestly `false`
- [x] Cancel sends `turn/interrupt {threadId, turnId}`; a structured turn error is reported without
      ending the session
- [x] The thread id is stored as the resume token and reused with `thread/resume {threadId}` so a restart
      continues the same conversation (no replay is assumed)
- [x] Unknown methods and notifications are logged and ignored; protocol drift never crashes a session
- [x] `fixtures/harnesses/codex_fake.mjs` covers happy path, approval, diff, error, crash; the shared
      conformance suite passes
- [x] Capabilities accurately report diffs, permissions, and resume
- [x] Registered in `builtin_registry()`; doctor recognises the `codex` binary

## Resolution

Merged as `feat(adapters): add the Codex app-server adapter with fixture and conformance target`.

Decisions worth knowing:

- Implements the **v2 thread/turn API** (`initialize` → `initialized` → `thread/start` → `turn/start`,
  `item/*` notifications, `turn/diff/updated` for one aggregated diff per turn, `turn/completed`), not the
  removed v1 methods the original ticket text named.
- Approval answers use the minimal `accept`/`decline`/`cancel` vocabulary and the fixture pins it, so a
  protocol change fails a test instead of hanging a turn. Non-approval server requests
  (`requestUserInput`, elicitation) are answered `-32601` so the harness is never left waiting.
- Usage parsing accepts the several token-breakdown spellings releases have used and skips an unknown shape
  rather than reporting `0/0`.
- Turn-scoped frames are matched by `turnId` with the last ended turn remembered: stress-testing exposed a
  genuine flake where an interrupted turn's late `turn/completed` could end the *next* turn.
- `thread/start` sends only cwd + model — Grove deliberately does not choose a sandbox or approval policy on
  the user's behalf.
- A resume error (stale/unknown thread) falls back to `thread/start` with a warning instead of bricking the
  session.
- The shared conformance suite gained the opt-in `Target::with_permissions()` roundtrip (request emitted →
  answer delivered → `PermissionResolved`), replaced the old blanket refusal of permission claims, and the
  ACP target now opts in.

Not verified: no real `codex` binary was run (fixtures only); `approvalPolicy`/`sandbox` defaults and a real
app-server's stdin-EOF termination are untested (the process-tree kill is the backstop).
