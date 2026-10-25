# 09: TODO plan — one versioned plan per session, editable by human and agent

**What to build:** Each session has a plan the user edits in a panel and the agent edits as a file. Both
survive restarts, and neither clobbers the other: a stale save is rejected as a conflict and the plan is
re-read.

**Context pointers:** `architecture.md` §Persistence (plan dual-edit). A mirror file is the agent-facing
surface; SQLite stays the record.

**Blocked by:** 02 (session core)

**Status:** done

- [x] `PlanService` get / set with a version check; a stale write returns `conflict`; two concurrent
      writers are covered by a test
- [x] Every write mirrors `.grove/sessions/<id>/plan.md` in the session's project, atomically, creating
      directories on first write
- [x] An external edit to the mirror (the agent) is imported by a watcher when its hash changed and the DB
      has not moved since the last mirror write; the import bumps the version and emits
      `grove://plan-changed`
- [x] The plan survives a restart, including an external edit made while the app was closed
- [x] Plan panel: markdown editor plus preview, save with conflict handling, shows the mirror path with a
      copy affordance
- [x] `plan_get` / `plan_set` documented in contract fixtures; tests run against a tempdir project
- [x] The plan text is exposed to session startup for adapters to consume (ticket 14 wires it into the
      prompt)

## Resolution

Merged as `feat(plan): versioned per-session TODO plan with atomic agent mirror and Plan panel`.

Decisions worth knowing:

- No native file watcher. External (agent) edits are detected by a hash check before every read
  (`get`, `text`, and the import step of `set`), with a pure `decide_import` table: empty, in sync,
  import, or restore-mirror. The panel re-reads on focus and on a short poll, which is what makes an
  agent edit appear without a filesystem-timing test.
- The `plans` row gained `mirror_hash` so Grove can tell its own mirror write from an external edit;
  `architecture.md` was updated to record both this and the read-time check.
- The command surface stays at the two documented commands: `plan_get` already imports, so a separate
  refresh command would be redundant.
- `Plan::sessionId` is on the DTO so results are self-describing; `PlanService::text()` is the accessor
  ticket 14 will use when assembling session context.

Not verified: native delivery of `grove://plan-changed` in a running window, and a real agent writing
the mirror end-to-end (covered at the filesystem level).
