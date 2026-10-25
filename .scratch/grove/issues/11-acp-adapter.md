# 11: ACP adapter — any Agent Client Protocol agent

**What to build:** A user points Grove at any ACP-speaking agent (command + args) and gets a normal
session: streamed messages, tool calls, permission prompts, cancellation, and resume. No
harness-specific code — this adapter is the proof that the ACP path is generic.

**Context pointers:** `architecture.md` §The adapter contract, §Normalized event model.
`docs/adapters.md` from ticket 08. **Protocol reference:** `C:/Users/zyko8/.grove-notes/protocols.md` §3
(ACP) — pin `protocolVersion: 1`; v2 is draft with renames.

**Blocked by:** 08 (adapter contract + Claude Code adapter)

**Status:** done

- [x] Command and args come from config; with no command configured, `detect` reports not installed with
      a clear reason and starting fails as `unavailable` (not a panic)
- [x] JSON-RPC over stdio: `initialize` with `protocolVersion: 1`, `session/new` with cwd and MCP server
      list, `session/prompt` with prompt blocks, `session/cancel`
- [x] Resume uses `session/resume` (or suppresses replay), never plain `session/load`: ACP replays the
      whole conversation as updates and would duplicate Grove's append-only transcript
- [x] `session/update` notifications map to normalized events: agent message chunks, tool call + tool
      update, and safely ignored plan updates
- [x] `session/request_permission` is a request that is always answered: an approval returns
      `{outcome: {outcome: "selected", optionId}}`, and a cancelled turn returns
      `{outcome: {outcome: "cancelled"}}` so the agent never hangs waiting
- [x] A protocol-version rejection produces a clear error naming both versions, and the session ends with
      `status error` rather than hanging
- [x] The ACP session id is the resume token and is reused on restart
- [x] `fixtures/harnesses/acp_fake.mjs` covers happy path, permission, cancel, version mismatch; the shared
      conformance suite passes
- [x] Capabilities are accurate for what the fake exercises; unknown methods are logged and ignored
- [x] Registered in `builtin_registry()`; doctor reports it as configurable rather than auto-detected

## Resolution

Merged as `feat(adapters): add the generic ACP adapter over stdio JSON-RPC`.

Decisions worth knowing:

- `capabilities.usage = false`: ACP's `usage_update` carries context-window `used`/`size` and cost, not the
  input/output split the normalized `Usage` needs. Mapping `used` to input tokens would have invented a
  number.
- `capabilities.mcp = false` although `mcpServers` is forwarded (the protocol requires the field) — Grove's
  catalog is not wired in, so claiming it would be dishonest.
- Permissions are always answered: `selected` with the best-matching option, `cancelled` when there is no
  match or the turn was cancelled. An unanswered request would hang a real agent.
- Resume uses `session/resume`; the fake exits non-zero on `session/load`, so a future regression toward the
  replaying method fails loudly.
- Cancel is a notification, so the adapter waits for the prompt's `cancelled` stop reason with a timeout and
  then ends the turn locally — the UI can never hang.
- The shared conformance suite gained a `permission` scenario and now requires an adapter that claims
  `permissions` to prove it emits a request, accepts the answer, and records the resolution.

Not verified (code paths without fixture modes): the cancel-timeout fallback, `refusal`/`max_tokens` stop
reasons, the `authMethods` error path, and the "agent cannot resume ⇒ fresh session" fallback. Also
unverified: real agents (tests use the fake), and Content-Length-framed JSON-RPC (the fixture and notes use
newline-delimited JSON).
