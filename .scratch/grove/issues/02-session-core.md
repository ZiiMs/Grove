# 02: Session core — normalized events, SQLite log, mock harness, restart survival

**What to build:** A user creates a session, picks the built-in mock harness, sends a message, watches a
streamed reply with a tool-call card, cancels it, then quits and reopens Grove and finds the session and
its full transcript intact. Sessions are the unit every other feature hangs off.

**Context pointers:** `architecture.md` §Normalized event model, §Session runtime, §Persistence.
Ticket 01 created the shell, bridge, and contract package.

**Blocked by:** 01 (foundation shell)

**Status:** ready-for-agent

- [ ] `SessionManager` supports create / send / cancel / close / resume; commands for one session are
      serialized so two never interleave
- [ ] Every event is persisted in order with a sequence number; reopening the DB returns the same
      transcript
- [ ] Mock adapter streams `delta` → `message` → `tool_call`/`tool_result` → `status idle`, honours
      cancel, exposes a resume token, and is registered as `mock`
- [ ] Restart test: run a session, drop the manager, reopen the store, resume, continue the conversation
- [ ] SQLite schema and ordered migrations per architecture; paths resolved through `AppPaths`
- [ ] `Session`, `SessionSummary`, `SessionDetail`, `HarnessEvent` have contract fixtures validated on
      both sides
- [ ] Sessions panel: session list, create with a harness picker limited to registered harnesses,
      virtualized conversation with tool-call cards, compose box, cancel; unit-tested with `FakeBridge`
- [ ] Events stream to the UI over `grove://session-event` and rebuild the conversation after a reload
- [ ] No test touches the network; the adapter contract is untouched by this ticket
