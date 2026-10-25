# 0002 — Workspace projects and the redesigned shell

- **Status:** accepted (second build)
- **Date:** 2026-10-25
- **Supersedes:** `architecture.md` §Rendering, the seven-panel shell described by the first build,
  and the retired Specs/Plan editor surfaces.

## Context

The first build was a seven-panel application: a sidebar switched Sessions, Skills, Specs, Plan, MCP,
Git and Settings, and the centre was whichever panel was selected. It proved the core, the adapters and
the IPC contract, but it made the product feel like six tools behind one window: the chat disappeared
when you looked at your plan, the tree had no notion of a project, and the Git panel was a form that
asked for a repository path.

The design handoff in `design-reference/` settles a different shell around the same core: a project
tree, an always-mounted conversation, a per-session context column, and one near-fullscreen settings
sheet that owns the catalog, harnesses, MCP and themes. It also introduces seven visual languages with
independent dark and light columns. The handoff assumes data the first build did not model — projects,
worktrees as first-class tree nodes, which sessions are waiting on a human, and writable git staging.
This ADR records the decisions that made the export implementable without inventing fixture data.

## Decisions

### 1. Projects are durable rows; worktrees are read from git

`projects(id, name, path, remote, default_branch, is_git, created_at, updated_at)` lets the tree
remember a folder the user added, even before any session exists. A session stores `project_id`
(nullable: a chat with no project) and the store backfills it when a project is added by matching the
session's `cwd` against the project path and its worktrees.

Worktrees are deliberately **not** a table. Git already owns worktree state, and a stored copy goes
stale the moment the user runs `git worktree add` in a terminal. The tree reads `git worktree list` per
project and groups sessions by the checkout they run in, so external git work is reflected on the next
refresh instead of fighting the app.

### 2. A session carries the one bit the tree needs

`awaiting` is mirrored into `sessions` by the pump on `permission_request` / `permission_resolved`
(and cleared by `exited`/`error`). The tree, titlebar and status bar therefore show "needs you"
without loading a transcript per row. The transcript remains the authority for *what* was asked.

### 3. The shell is three columns, and the centre never changes

| Column | Content |
|---|---|
| 272px | `Chats` (sessions with no project) and `Projects → worktrees → sessions`, filter, add-project, per-node new-session |
| 1fr | The active conversation, its compact header and composer |
| 304px | Per-session context: Git · Plan · Spec, collapsible to a stub |

Skills, MCP, harness health and appearance moved into one settings modal opened from the tree footer.
Selecting a project, worktree or session changes the centre; it never swaps the product surface. Below
900px the three columns become Navigation · Session · Context tabs instead of shrinking to unusable
widths.

### 4. Read-only spec and plan views replace the editors

The export's context column shows the plan as a checklist and a spec as a peek card, not an editor.
Keeping two editors for surfaces the design does not show would have produced dead weight the user
never finds. `spec_read` still supplies the preview, and the Spec tab can pin a spec to the session;
authoring specs remains a file task. Monaco, `markdown-it` and their lazy chunks are gone. The
transcript is still virtualized with `@tanstack/react-virtual`, because a long session is the one list
that genuinely grows without bound.

### 5. The missing backend is part of this change

The design's dialogs needed operations the first build did not have. They are real commands now, not
disabled controls or prototype fixtures:

- `project_list`, `project_detect`, `project_add` — add a folder, detect git, keep it.
- `fs_list_dirs` — one level of directories for the add-project picker.
- `git_worktree_create` — create a branch plus its checkout when a new session asks for a worktree.
- `git_stage` — stage/unstage selected paths, because the changes list shows git's truth.
- `git_open_pr` gained `draft`; the dialog defaults to a ready PR and only passes `--draft` when asked.
- `skill_installs`, `skill_remove` — the catalog can mark installed skills and delete them safely.

`session_create` accepts `projectId` and `worktreePath`; the working directory resolves from them,
falling back to the old `cwd` rule for a loose chat. `session_switch_harness` accepts a model so the
composer's single picker can change model inside a harness as well as across harnesses.

### 6. The exported visual contract is the token source

`design-reference/grove-themes.css` became `apps/desktop/src/styles/themes.css` unchanged, and the
export's component CSS became `styles/design.css`. Component code consumes those tokens only. The
seven theme fonts are vendored as latin WOFF2 subsets so every theme renders offline; the theme
choice is applied before first paint and persisted through settings. The old Ember token file and the
per-panel stylesheets are deleted rather than kept beside the new system.

### 7. The bundle guard becomes a budget

With no editor in the webview, "Monaco must be lazy" has no referent. `scripts/check-bundle.mjs` now
fails if Monaco reappears in the entry chunk or if the entry chunk exceeds 600 kB, and reports the
largest lazy chunks. The performance promise stays machine-checked; the rule just matches what the app
loads.

## Consequences

- Existing installs migrate in place: migration 5 adds `projects` and the two session columns. Added
  projects adopt sessions that already ran in their checkout.
- The tree is the navigation. There is no panel registry to add to; a new **surface** is a component
  mounted from `App.tsx`, and a new **command** is one module plus one line in `commands/mod.rs`.
- Worktree creation is the one place the UI writes to a path outside the repository. The core refuses
  a non-empty folder and validates the refs; the path is otherwise the user's choice.
- The visual harness (`apps/desktop/tools/visual.test.tsx`, `npm run visual -w @grove/desktop`) renders
  the built shell into `dist/visual-*.html` for screenshot comparison. It is deliberately outside
  `npm test`, because it needs a production build.
- A future editor surface (for specs, plans or diffs) has to earn its place in the context column; the
  read-only views are the baseline.

## Alternatives considered

| Decision | Alternative | Why not |
|---|---|---|
| Durable projects, live worktrees | Store worktrees too | A checked-out worktree changes outside Grove; a stale table is worse than a git read |
| One shell, always-chat centre | Keep seven panels and add a tree | The designer's premise is that context belongs *beside* the conversation, not in place of it |
| Read-only context views | Keep Monaco spec/plan editors hidden behind a menu | Two editors outside the design's surface are untested weight and an inconsistent interaction model |
| Real backend for projects/worktrees | Port with design fixtures, wire later | The app would ship screens that cannot create the thing they show |
| Vendored latin fonts | `fonts.googleapis.com` link | Grove is local-first and must render its themes offline; a font fetch is a launch dependency |
| Entry budget guard | Drop the bundle guard | The ~200 MB idle goal is a release promise; removing the only machine check would make it aspirational |
