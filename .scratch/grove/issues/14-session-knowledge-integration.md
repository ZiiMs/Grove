# 14: Session knowledge integration — side-by-side sessions, pinned specs, plan in context

**What to build:** Two sessions, two harnesses, one window. The user pins the specs that matter and the
session starts with them plus the current plan. Switching a session's harness keeps its transcript and its
context. This is the goal spec's key user story, end to end.

**Context pointers:** `architecture.md` §Session runtime, §Persistence (pins, plan), §The adapter contract
(`StartContext.context_files`). Depends on the session, spec, plan, and Claude adapter work.

**Blocked by:** 02, 03, 08, 09

**Status:** done

- [x] Multiple sessions open side by side as tabs, each with its own transcript, compose state, and status;
      only the active session's event list is rendered
- [x] Specs can be pinned to a session from the Specs panel; pins persist in `pins`, survive restart, and
      appear in the session header
- [x] Session start/resume builds `ContextDoc`s from pinned specs (id, title, body; truncated with an
      explicit marker over budget) plus the current plan, and passes them to the adapter; a test asserts
      the exact context the fake harness receives
- [x] Context is snapshotted at start: later spec edits do not silently change a running session; a
      Refresh context action re-sends it and is reflected in the transcript
- [x] Switching a session's harness re-launches it with the same context docs and a transcript summary,
      appends a visible system note, and never loses the transcript
- [x] UI tests: two sessions side by side, pin a spec, start, see context, switch harness, transcript intact
- [x] No unbounded context: per-doc and total character budgets are enforced and tested

## Resolution

Merged as `feat(sessions): pin specs into session context and switch harnesses`.

Decisions worth knowing:

- **One new event kind** (`context_refreshed` with a reason: start / resume / refresh / harness_switch) rather
  than one variant per action. It records ids, titles, sizes and truncation — never the body, so the
  transcript does not duplicate every pinned spec. `packages/contract/src/core.ts` had to gain the kind in
  `eventKinds`, or the webview's envelope router silently dropped it (a failing UI test caught that).
- **Refresh pushes context as a normal turn** (`UserInput::text(render_documents(..))`): the adapter contract
  has no "update context" command and changing it mid-flight would have touched tickets 10–13. The visible
  note is written through the pump with an ack under the session command lock, preserving the one-writer rule.
- **Context is a snapshot at start** with explicit budgets (8 000 chars/doc, 24 000 total) and a truncation
  marker; a document too small to fit a marker is dropped and counted rather than emitted mangled.
- Pins live in `pins` (migration 4) in insertion order; an unknown spec id is rejected as `invalid`, while a
  pin whose spec was deleted keeps its header entry and contributes no document.
- `switch_harness` clears `model` and `resume_token` — both belong to the harness that produced them — and
  passes the same documents plus a transcript summary so the new harness is not starting from zero.
- The assembler reads the plan via `PlanService::get` (not `text`) so one read yields markdown *and* version
  for the header.

Not verified: no GUI smoke run (the Tauri command layer is type-checked; behaviour is proven at the
`SessionManager` level with a recording adapter).
