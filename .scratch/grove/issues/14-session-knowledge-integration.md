# 14: Session knowledge integration — side-by-side sessions, pinned specs, plan in context

**What to build:** Two sessions, two harnesses, one window. The user pins the specs that matter and the
session starts with them plus the current plan. Switching a session's harness keeps its transcript and its
context. This is the goal spec's key user story, end to end.

**Context pointers:** `architecture.md` §Session runtime, §Persistence (pins, plan), §The adapter contract
(`StartContext.context_files`). Depends on the session, spec, plan, and Claude adapter work.

**Blocked by:** 02, 03, 08, 09

**Status:** ready-for-agent

- [ ] Multiple sessions open side by side as tabs, each with its own transcript, compose state, and status;
      only the active session's event list is rendered
- [ ] Specs can be pinned to a session from the Specs panel; pins persist in `pins`, survive restart, and
      appear in the session header
- [ ] Session start/resume builds `ContextDoc`s from pinned specs (id, title, body; truncated with an
      explicit marker over budget) plus the current plan, and passes them to the adapter; a test asserts
      the exact context the fake harness receives
- [ ] Context is snapshotted at start: later spec edits do not silently change a running session; a
      Refresh context action re-sends it and is reflected in the transcript
- [ ] Switching a session's harness re-launches it with the same context docs and a transcript summary,
      appends a visible system note, and never loses the transcript
- [ ] UI tests: two sessions side by side, pin a spec, start, see context, switch harness, transcript intact
- [ ] No unbounded context: per-doc and total character budgets are enforced and tested
