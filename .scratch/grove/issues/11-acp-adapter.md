# 11: ACP adapter — any Agent Client Protocol agent

**What to build:** A user points Grove at any ACP-speaking agent (command + args) and gets a normal
session: streamed messages, tool calls, permission prompts, cancellation, and resume. No
harness-specific code — this adapter is the proof that the ACP path is generic.

**Context pointers:** `architecture.md` §The adapter contract, §Normalized event model.
`docs/adapters.md` from ticket 08.

**Blocked by:** 08 (adapter contract + Claude Code adapter)

**Status:** ready-for-agent

- [ ] Command and args come from config; with no command configured, `detect` reports not installed with
      a clear reason and starting fails as `unavailable` (not a panic)
- [ ] JSON-RPC over stdio: `initialize` with protocol-version negotiation, `session/new` with cwd and
      MCP server list, `session/prompt` with prompt blocks, `session/cancel`
- [ ] `session/update` notifications map to normalized events: agent message chunks, tool call + tool
      update, and safely ignored plan updates; `session/request_permission` becomes `permission_request`
      and the decision is returned in the agent's expected shape
- [ ] A protocol-version rejection produces a clear error naming both versions, and the session ends with
      `status error` rather than hanging
- [ ] The ACP session id is the resume token and is reused on restart
- [ ] `fixtures/harnesses/acp_fake.mjs` covers happy path, permission, cancel, version mismatch; the shared
      conformance suite passes
- [ ] Capabilities are accurate for what the fake exercises; unknown methods are logged and ignored
- [ ] Registered in `builtin_registry()`; doctor reports it as configurable rather than auto-detected
