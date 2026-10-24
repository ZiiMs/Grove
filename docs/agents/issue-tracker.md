# Issue tracker: Local Markdown

Issues and specs for this repo live as markdown files in `.scratch/`.

## Conventions

- One feature per directory: `.scratch/<feature-slug>/`
- The spec is `.scratch/<feature-slug>/spec.md` — for Grove the spec of record is the repo-root
  `goal-and-requirements.md` + `architecture.md`, which `.scratch/grove/spec.md` points at
- Implementation issues are one file per ticket at `.scratch/<feature>/issues/<NN>-<slug>.md`, numbered
  from `01`, never a single combined tickets file
- Triage state is recorded as a `Status:` line near the top of each issue file (see
  `triage-labels.md` for the role strings)
- Blocking edges are a `Blocked by:` line listing ticket numbers, or "None (can start immediately)"
- Comments and conversation history append to the bottom of the file under a `## Comments` heading
- Tickets are committed with the repo; `.grove/` local runtime state is not

## When a skill says "publish to the issue tracker"

Create a new file under `.scratch/<feature-slug>/issues/` (creating the directory if needed).

## When a skill says "fetch the relevant ticket"

Read the file at the referenced path. The user will normally pass the path or the issue number
directly.

## Closing work

A ticket is closed by setting `Status: done`, ticking its acceptance criteria, and appending a short
`## Resolution` note with the merge commit and the checks that proved it. Tickets are never deleted.

## Wayfinding operations

Used by `/wayfinder`. The **map** is a file with one **child** file per ticket.

- **Map**: `.scratch/<effort>/map.md` (the Notes / Decisions-so-far / Fog body).
- **Child ticket**: `.scratch/<effort>/issues/NN-<slug>.md`, with a `Type:` line
  (`research`/`prototype`/`grilling`/`task`) and a `Status:` line (`claimed`/`resolved`).
- **Frontier**: open, unblocked, unclaimed files; first by number wins. Claim before any work.
- **Resolve**: append the answer under `## Answer`, set `Status: resolved`, and add a context pointer
  to the map's Decisions-so-far.
