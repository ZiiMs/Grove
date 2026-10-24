# Grove — spec pointer

The spec of record for this effort lives at the repo root:

- `goal-and-requirements.md` — intent, users, goals, non-goals, success conditions
- `architecture.md` — repository shape, adapter contract, event model, persistence, IPC, sidecar
  protocol, rendering, testing, extension seams

This directory holds the implementation tickets for the first build:
`.scratch/grove/issues/NN-<slug>.md`, one ticket per file, blockers numbered first.

Working the graph means taking the **frontier**: any ticket whose every `Blocked by:` ticket has
`Status: done`.

## Ticket index

| # | Ticket | Blocked by |
|---|---|---|
| 01 | Foundation shell | — |
| 02 | Session core | 01 |
| 03 | Spec graph | 01 |
| 04 | Git panel | 01 |
| 05 | Skill catalog | 01 |
| 06 | MCP viewer | 01 |
| 07 | Settings, secrets, doctor | 01 |
| 08 | Adapter contract + Claude Code adapter | 02 |
| 09 | TODO plan | 02 |
| 10 | Codex adapter | 08 |
| 11 | ACP adapter | 08 |
| 12 | OpenCode adapter | 08 |
| 13 | pi adapter + Node sidecar | 08 |
| 14 | Session knowledge integration | 02, 03, 08, 09 |
| 15 | Hardening and release readiness | 04–07, 10–14 |
