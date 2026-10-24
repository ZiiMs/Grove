# 04: Git panel — status, branches, worktrees, diff, commit, draft PR

**What to build:** A user opens the Git panel, sees the current branch and what changed, reads a diff,
commits the files they pick, and opens a draft PR from the same panel — without a terminal.

**Context pointers:** `architecture.md` §Subsystems (gitops), §Windows-first (arg handling). `git` and
`gh` are the real tools; both may be absent, which must be reported, not assumed.

**Blocked by:** 01 (foundation shell)

**Status:** done

- [x] gitops discovers the repo root, parses status, lists branches (local/remote, ahead/behind), lists
      worktrees, produces unified diffs for the working tree and for a branch against a base, and commits
      selected paths with a message
- [x] `gh pr create --draft` takes a title/body and returns the URL; when `gh` is missing or
      unauthenticated the command returns `unavailable` with the reason — never a panic or a silent no-op
- [x] Every subprocess goes through an injectable command runner (no shell strings; args as a list) so
      tests drive a fake `gh` and a real temp git repo
- [x] Tests cover: clean tree, dirty tree, staged file, new branch, linked worktree, commit, diff body,
      absent binary, large diff truncation marker
- [x] Git panel: status list, current branch, diff viewer with Monaco lazy-loaded, commit form with path
      selection, PR button that surfaces the URL or the unavailable reason
- [x] Output is bounded and streamed/truncated with a clear marker; no command writes outside the repo
- [x] Contract fixtures for `GitStatus`, `GitBranch`, `GitWorktree`, `GitDiff`, `PrResult`

## Resolution

Shipped in `feat(git): add gitops, the git panel, and the draft-PR flow`.

Decisions worth knowing:

- gitops is synchronous behind one `CommandRunner` trait (`CommandSpec { program, args, cwd, env }`),
  and the Tauri handlers call it through `spawn_blocking`, so the runner seam stays trivially testable
  while the webview's command queue never waits on a large diff.
- A commit stages the selected pathspecs (`git add --all -- <paths>`) and then uses
  `git commit -- <paths>`, so unrelated staged changes stay staged and untracked selections work.
  Paths are validated as repository-relative before any process starts.
- Output is bounded at 256 KiB. Diffs carry a trailing `--- output truncated: N of M bytes omitted ---`
  marker plus `truncated: true`; a pathological status is cut on a NUL-record boundary and flagged.
- `gh` argv is `pr create --draft --title <title> --body <body> [--base b] [--head h]` with
  `GH_PROMPT_DISABLED=1`; auth failures map to `unavailable`, other refusals to `invalid` with stderr.
- The diff viewer is Monaco's `Editor` in `diff` language (a unified patch is not a Monaco diff model),
  imported through `React.lazy`, and `contract/fixtures/git-changed.json` pins the commit event payload.

Not verified: launching the GUI and observing the painted panel — this environment cannot display one.
The panel is covered by jsdom tests and the shell by `cargo check`.
