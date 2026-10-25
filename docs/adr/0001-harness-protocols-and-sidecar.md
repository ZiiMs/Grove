# 0001 — Harness protocols, the sidecar boundary, and context as a snapshot

- **Status:** accepted (first build)
- **Date:** 2026-10-24
- **Supersedes:** the tentative choices in `architecture.md` §The adapter contract, §Sidecar protocol
  and §Session knowledge where they differ; those sections now record the decisions below.

## Context

The first build had to make five harnesses (Claude Code, Codex, ACP agents, OpenCode, `pi`) behave as
one interface without pretending their protocols are the same. Several early plans were written before
the protocols were exercised against fixtures, and four of them changed during the build. This ADR
records what shipped and why, so a later ticket does not "restore" a plan that was deliberately
dropped.

## Decisions

### 1. Codex: the v2 thread/turn app-server API, not the removed v1 methods

Codex's app-server now speaks `initialize` → `initialized` → `thread/start` → `turn/start`, with
`item/*` notifications, `turn/diff/updated` for one aggregated diff per turn, and `turn/completed`.
The ticket text named v1 methods that no longer exist. The adapter implements v2 only and the fixture
pins the shapes (`fixtures/harnesses/codex_fake.mjs`), so a protocol change fails a test instead of
hanging a turn. Turn-scoped frames are matched by `turnId` because a late `turn/completed` from an
interrupted turn otherwise ended the next turn.

### 2. Claude Code: one process per turn, not long-lived stream-input mode

The adapter runs `claude --print --output-format stream-json --verbose` per turn and uses
`--resume <session_id>` for follow-ups. This is the documented headless form; cancellation is a
process-tree kill, and the adapter does not depend on the undocumented control-request channel.
The honest cost is recorded in `capabilities`: one-shot `-p` has no answerable permission channel
(`permissions: false`) and the CLI emits no patch (`diffs: false`). A live process with an interactive
permission channel is future work behind the same `HarnessSession` trait.

### 3. `pi`: drive RPC mode through a Node sidecar, not an in-process SDK

`pi` is JS-first, but importing its SDK into Rust would put a JavaScript package layout inside the
core and make the adapter untestable without a real install. Instead the sidecar owns pi's wire
format and speaks **Grove Sidecar Protocol v1** (`docs/sidecar-protocol.md`): NDJSON JSON-RPC on
stdio, already-normalized `HarnessEvent` notifications, versioned by `protocolVersion: 1`. The sidecar
runs `pi --mode rpc` with LF-only JSONL framing (pi's message text can contain U+2028/29, so Node's
`readline` is deliberately not used), and the Rust `pi` adapter is a host for the protocol, tested
against `fixtures/harnesses/pi_sidecar_fake.mjs`. An SDK path would sit *behind* the protocol and
change nothing above it; it is not built. The protocol document is registered as a spec node
(`grove-sidecar-protocol`) so it is part of the architecture graph, not a loose note.

### 4. Plan dual-edit: a read-time hash check, not a native file watcher

SQLite is the record and `.grove/sessions/<id>/plan.md` is the agent-editable mirror. A `notify`
watcher was the earlier plan, but it makes correctness a filesystem-timing question (editor temp
renames, duplicate events, Windows watcher latency) and adds a background task per session. Instead
`PlanService` compares the file's hash with the record and `plans.mirror_hash` before every read
(`get`, `text`, and the import step of `set`), with a pure four-way decision: empty, in sync, import an
external edit (version bump + `grove://plan-changed`), or restore a stale/missing mirror. The panel
also re-reads on focus and a short poll, which is what makes an agent edit visible without a watcher.
Stale writes are refused with `conflict`, so neither side clobbers the other.

### 5. Session knowledge: an explicit snapshot at start/resume/refresh, not live re-reading

Pinned specs and the plan are assembled into `ContextDocument`s at start, resume and harness switch,
then handed to the harness once. A per-document budget (8 000 chars) and a total budget (24 000
chars) with an explicit truncation marker keep the prompt bounded; a document too small to fit a
marker is dropped and counted. Nothing re-reads a spec on a timer: a running conversation cannot
silently change under the user. `session_refresh_context` is the explicit way to move the snapshot,
and it is visible twice — the harness receives the rendered documents as a turn, and the transcript
records a `context_refreshed` note (ids, titles, sizes, truncation; never the bodies).

### 6. Permission conformance is opt-in and scripted

An adapter may only claim `capabilities.permissions` when its fixture scripts a permission roundtrip
and its conformance target opts in with `Target::with_permissions()`. This is why ACP and Codex claim
the capability and Claude, OpenCode and pi do not: a claim is a test, not an aspiration.

## Consequences

- `architecture.md` and `docs/adapters.md` describe these decisions; any future change updates them
  and adds a superseding ADR.
- The core has no dependency on a harness SDK. Adding a sixth harness is one module, one registry
  line, one fixture and one conformance/restart target.
- Resume tokens are opaque strings: Claude's `session_id`, Codex's thread id, an OpenCode/ACP session
  id, or pi's session file path. Nothing may assume a UUID.
- The first build does not use a filesystem watcher for plans or specs on the session path; spec
  watching (panel freshness) remains, plan correctness does not depend on it.

## Alternatives considered

| Decision | Alternative | Why not |
|---|---|---|
| Codex v2 | Keep the v1 method names from the ticket | They were removed from the app-server; fixtures would test a protocol nobody serves |
| Claude per-turn | Long-lived stream-input with control requests | Undocumented, version-dependent, and a hung control channel is worse than a process kill |
| pi sidecar | Import the JS SDK / spawn pi directly from Rust | Puts a package layout in the core, breaks the protocol boundary, and cannot be tested without pi |
| Plan hash check | `notify` watcher | Timing-dependent correctness, extra task per session, noisy on Windows editors |
| Context snapshot | Re-read pins/specs per turn | Model input changes silently mid-conversation; costs a read on every turn for knowledge the user did not change |
