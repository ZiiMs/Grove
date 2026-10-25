# Issue tracker: ClickUp

Issues for this repo live as ClickUp tasks. Specs do not: the spec of record stays in the repo.

- Workspace `Workspace` — `9017632039`
- Space `Team Space` — `90172993258`
- Folder `Grove` — `901712062226`
- List `Tickets` — `901718007593`
  ([open in ClickUp](https://app.clickup.com/9017632039/v/l/li/901718007593))

The account holding this token has **two** workspaces and both are literally named `Workspace`, so a
name identifies nothing: pass `workspace_id: "9017632039"` on every `clickup_*` call. Without it the
tools fail with "Multiple workspaces available" instead of picking one — there is no safe default.

Every operation goes through the `clickup_*` MCP tools. Tickets are never written as files; `.scratch/`
holds history, not new work (see **History**).

## Conventions

- **Create a ticket**: `clickup_create_task` with `workspace_id: "9017632039"` and
  `list_id: "901718007593"`. The name is the deliverable in the imperative ("Add credential fallback
  to the vendor env var"). The body goes in `markdown_description` with these headings, in this
  order: `## Context`, `## What to build`, `## Acceptance criteria`, `## Out of scope`, `## Links`.
  A blocking edge is also a `Blocked by:` line at the very top (see **Blocking**). New tickets start
  tagged `needs-triage` unless the skill creating them already knows better; pass it in `tags` at
  creation. Tag names are lowercase and a name the space does not have yet is silently created, so
  copy the strings in `triage-labels.md` exactly rather than retyping them.
- **Read a ticket**: `clickup_get_task` with
  `include: ["description", "dependencies", "subtasks", "custom_fields"]`.
- **List tickets / frontier query**: `clickup_filter_tasks` with
  `workspace_id: "9017632039"`, `list_ids: ["901718007593"]`,
  `fields: ["custom_id", "status", "tags", "assignees", "date_updated"]`, plus `tags`, `statuses` or
  `assignees` as filters, and `include_closed: false` unless resolved tickets matter. `fields` is a
  fixed enum, not free text: `id` or `name` there is rejected, and both come back anyway.
- **Status**: the list has exactly three — `to do`, `in progress`, `complete`. Triage lives in tags,
  so `to do` covers everything from untriaged to ready-for-agent.
- **Tags**: `clickup_add_tag_to_task` / `clickup_remove_tag_from_task`. Triage tags — five state, two
  category — are space-level in `Team Space`; see `triage-labels.md`. Tags used for a new purpose are
  created in the same change (`clickup_create_space_tag`), never invented per ticket.
- **Comment**: `clickup_create_comment` with `entity_type: "task"`, `entity_id` set to the task id,
  and markdown in `comment_text`.
- **Blocking**: the native dependency, `clickup_add_task_dependency` with `type: "waiting_on"` — the
  blocked ticket is `task_id`, its blocker is `depends_on`. The `Blocked by:` line in the body repeats
  it in text so the ticket still reads correctly in an export or a screenshot. A ticket is unblocked
  when every blocker is `complete`.
- **Close**: see **Closing work**. A ticket is closed by status, never deleted.

Task ids are long numbers. If the workspace has custom ids enabled, quote those (`GRV-123`); otherwise
quote the task id. Either way, when reporting a ticket, link it as
`[<ticket name>](https://app.clickup.com/t/<task id>)`.

## History

`.scratch/grove/` holds the first build's ticket graph — `spec.md` plus fifteen `done` tickets under
`issues/` — and it stays as the record of why those pieces were built. It is read-only: new tickets go
to ClickUp, and history is not back-filled into ClickUp unless someone asks for it. The rest of
`.scratch/` is local scratch space (screenshots, dev browser profiles) and is not tracked.

## Rejected requests

`triage` keeps the reasoning for a rejected *enhancement* in `.out-of-scope/<concept>.md` at the repo
root — one file per concept, in the repo, not in ClickUp, so the decision survives the ticket. It does
not exist yet; `triage` creates it on the first rejection and reads the whole directory when
evaluating a new request. A `wontfix` closed because the work already exists gets no entry: the
closing comment points at where it lives.

## What a ticket is not

A ticket is the unit of work; it is not the spec. `goal-and-requirements.md` is the intent of record,
`architecture.md` is the authoritative design, and `docs/adr/` records why boundaries moved. A ticket
that implements part of a spec links to the file and section it implements — it does not restate it.
`CODING_STANDARDS.md` is read at review, not copied into tickets.

## When a skill says "publish to the issue tracker"

Create a task in `Tickets` as above, tagged `needs-triage`. One task per ticket: no combined
"here are all the tickets" task. If the tickets come from a spec, put the spec path in `## Links`.

## When a skill says "fetch the relevant ticket"

`clickup_get_task` by id or custom id. The user normally passes the id, the link, or the ticket name;
for a name, `clickup_search` resolves it.

## Lifecycle

1. **Before code**: a feature-sized request gets a ticket. The ticket is what the review stage reads,
   so a change reviewed only against a chat message has no spec axis at all.
2. **On start**: one `clickup_update_task` with `status: "in progress"` and
   `assignees: ["14712905"]` (Alex Tedesco). One write, at the start of the work.
3. **On finish**: comment the summary — what changed, where, what was verified, links — then set
   status `complete`. The same content goes to the user, so the ticket is readable without asking.

## Closing work

The closing comment carries the resolution; the ticket body is not rewritten. The comment names the
merge commit and the checks that proved the change, and it stays on the task forever. Triage tags are
left as they were: the status already says it is done.

## Wayfinding operations

Used by `/wayfinder`. The **map** is one task, the **children** are its subtasks.

- **Map**: a task in `Tickets` named `Wayfinder: <effort>`, tagged `wayfinder:map`, with the Notes /
  Decisions-so-far / Fog body in the description.
- **Child ticket**: a subtask of the map (`parent: "<map task id>"`), tagged `wayfinder:<type>`
  (`research` / `prototype` / `grilling` / `task`). Status is the state: `to do` open, `in progress`
  claimed, `complete` resolved. Both tag families are created on first use, not ahead of time.
- **Blocking**: `clickup_add_task_dependency` with `type: "waiting_on"`, child → blocker, plus the
  `Blocked by:` line. Unblocked when every blocker is `complete`.
- **Frontier**: open subtasks of the map that are unassigned, have no open blocker, and are not
  `complete`; subtask order decides, first wins.
- **Claim**: one `clickup_update_task` with `assignees: ["14712905"]` and `status: "in progress"`,
  before any work.
- **Resolve**: comment the answer, set `complete`, then append a context pointer (gist + link) to the
  map's Decisions-so-far.
