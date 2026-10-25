# 02: Session core — normalized events, SQLite log, mock harness, restart survival

**What to build:** A user creates a session, picks the built-in mock harness, sends a message, watches a
streamed reply with a tool-call card, cancels it, then quits and reopens Grove and finds the session and
its full transcript intact. Sessions are the unit every other feature hangs off.

**Context pointers:** `architecture.md` §Normalized event model, §Session runtime, §Persistence.
Ticket 01 created the shell, bridge, and contract package.

**Blocked by:** 01 (foundation shell)

**Status:** done

- [x] `SessionManager` supports create / send / cancel / close / resume; commands for one session are
      serialized so two never interleave
- [x] Every event is persisted in order with a sequence number; reopening the DB returns the same
      transcript
- [x] Mock adapter streams `delta` → `message` → `tool_call`/`tool_result` → `status idle`, honours
      cancel, exposes a resume token, and is registered as `mock`
- [x] Restart test: run a session, drop the manager, reopen the store, resume, continue the conversation
- [x] SQLite schema and ordered migrations per architecture; paths resolved through `AppPaths`
- [x] `Session`, `SessionSummary`, `SessionDetail`, `HarnessEvent` have contract fixtures validated on
      both sides
- [x] Sessions panel: session list, create with a harness picker limited to registered harnesses,
      virtualized conversation with tool-call cards, compose box, cancel; unit-tested with `FakeBridge`
- [x] Events stream to the UI over `grove://session-event` and rebuild the conversation after a reload
- [x] No test touches the network; the adapter contract is untouched by this ticket

## Resolution

Shipped on `ticket/02-session-core` (uncommitted in the worktree; the integration merge commit is
recorded when the parent merges).

Decisions worth knowing:

- **One writer per session.** The pump task is the only thing that appends to `events` and broadcasts
  on the bus while a session is live; manager-authored events (the user's message) are injected into
  that pump and acknowledged before the adapter is asked to run. Persist-then-broadcast means the log
  is never behind what the UI painted, and the stream order and row order cannot disagree. The
  manager writes directly only after it has stopped the pump (close/resume transitions).
- **Sessions are rows, not processes.** `SessionManager::open` marks `starting|ready|busy` rows
  `exited`, so the UI offers Resume instead of pretending a process survived the quit.
- **Resume token mirroring.** The token is written at start and after each command; `resume` hands it
  back through `StartContext.resume_token`, and adapters without `capabilities.resume` get the
  transcript tail as `StartContext.prompt` instead.
- **Migrations.** `MIGRATIONS` is an ordered `&[&str]`; migration 1 creates `sessions`, `events` and
  its index. Later domains (plans, pins, installs, settings) append migrations 2+ rather than editing
  1, so parallel tickets do not collide in the list.
- **Contract shape.** `Session`/`SessionSummary`/`SessionDetail` are camelCase and flatten the session
  row (`SessionSummary = Session + eventCount`, `SessionDetail = Session + events`); event payloads
  stay snake_case (`call_id`, `input_tokens`) as settled in §Normalized event model.
- **UI fold.** `foldEvents` turns the persisted log into conversation rows, deduplicating a streamed
  reply against the `message` that completes it — so the live stream and a reload render the same
  conversation, and tool results attach to their call as one card.
- `jsdom` has no layout: `src/test/setup.ts` gives elements a box and a no-op `ResizeObserver` so the
  virtualized list renders in tests.

Checks: `cargo test -p grove-core`, `cargo clippy -p grove-core --all-targets -- -D warnings`,
`cargo fmt --all -- --check`, `cargo check -p grove-desktop`, `npm run typecheck`, `npm test`,
`npm run build`.
