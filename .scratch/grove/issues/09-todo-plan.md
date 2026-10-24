# 09: TODO plan — one versioned plan per session, editable by human and agent

**What to build:** Each session has a plan the user edits in a panel and the agent edits as a file. Both
survive restarts, and neither clobbers the other: a stale save is rejected as a conflict and the plan is
re-read.

**Context pointers:** `architecture.md` §Persistence (plan dual-edit). A mirror file is the agent-facing
surface; SQLite stays the record.

**Blocked by:** 02 (session core)

**Status:** ready-for-agent

- [ ] `PlanService` get / set with a version check; a stale write returns `conflict`; two concurrent
      writers are covered by a test
- [ ] Every write mirrors `.grove/sessions/<id>/plan.md` in the session's project, atomically, creating
      directories on first write
- [ ] An external edit to the mirror (the agent) is imported by a watcher when its hash changed and the DB
      has not moved since the last mirror write; the import bumps the version and emits
      `grove://plan-changed`
- [ ] The plan survives a restart, including an external edit made while the app was closed
- [ ] Plan panel: markdown editor plus preview, save with conflict handling, shows the mirror path with a
      copy affordance
- [ ] `plan_get` / `plan_set` documented in contract fixtures; tests run against a tempdir project
- [ ] The plan text is exposed to session startup for adapters to consume (ticket 14 wires it into the
      prompt)
