# Glossary

Only the terms this codebase actually uses. Each one names the module that owns it, so the definition
can be checked against the implementation.

## Harness

A coding agent's own product and protocol: Claude Code, Codex, an ACP agent, OpenCode, `pi` — plus the
in-process `mock` used for offline demos and Grove's own tests. A harness owns its auth and its wire
format; Grove never proxies a model call. The fixed list lives in
`crates/grove-core/src/adapters/registry.rs`.

## Adapter

The **only** place one harness's protocol is known (`crates/grove-core/src/adapters/*.rs`). An adapter
starts the harness, translates its wire format into normalized `HarnessEvent`s, and turns user actions
back into the harness's own commands. It never imports another adapter, `tauri`, or UI code.

## Session

A row in `sessions` plus, while it is running, one harness process and one pump task
(`sessions::manager`). Sessions are rows, not processes: the transcript is in SQLite, a restart marks a
live row `exited`, and `session_resume` restarts the adapter with the stored resume token. The
conversation survives a quit.

## Spec graph

The index of repo `.md` files whose frontmatter carries `id` + `type`, with `parent`, `depends-on`,
`references`, `implements` and `covers` edges (`specgraph/`). It powers the Specs panel, validates
dangling links/duplicate ids/parent cycles, and is the source of the spec node a session can pin.

## Plan mirror

The agent-editable copy of a session's TODO plan at `.grove/sessions/<id>/plan.md` (`plan/`). SQLite is
the record; every write also writes the mirror atomically. `plans.mirror_hash` records what Grove last
wrote, so a read can tell an agent's external edit (import, version bump) from Grove's own write
(rewrite if stale or missing). There is no native file watcher: the check happens at read time.

## Sidecar

A process that speaks **Grove Sidecar Protocol v1** on stdio (`docs/sidecar-protocol.md`), used when a
harness's only stable surface is a JavaScript SDK or RPC CLI. The first implementation drives
`pi --mode rpc`; the Rust `pi` adapter is its host, and the protocol is swappable without changing the
core. It is bounded by `sidecar/src/` and `adapters/pi.rs`.

## Context document

One pinned spec or the current plan, already rendered to markdown, that a session start/resume/refresh
hands to the harness (`sessions::context::{ContextDocument, ContextSource}`). Context is a **snapshot**
with explicit budgets (8 000 chars per document, 24 000 total) and a truncation marker; nothing
re-reads a spec on a timer. The transcript records what was sent as a `context_refreshed` event.
