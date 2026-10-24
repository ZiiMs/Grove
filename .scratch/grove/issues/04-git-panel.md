# 04: Git panel — status, branches, worktrees, diff, commit, draft PR

**What to build:** A user opens the Git panel, sees the current branch and what changed, reads a diff,
commits the files they pick, and opens a draft PR from the same panel — without a terminal.

**Context pointers:** `architecture.md` §Subsystems (gitops), §Windows-first (arg handling). `git` and
`gh` are the real tools; both may be absent, which must be reported, not assumed.

**Blocked by:** 01 (foundation shell)

**Status:** ready-for-agent

- [ ] gitops discovers the repo root, parses status, lists branches (local/remote, ahead/behind), lists
      worktrees, produces unified diffs for the working tree and for a branch against a base, and commits
      selected paths with a message
- [ ] `gh pr create --draft` takes a title/body and returns the URL; when `gh` is missing or
      unauthenticated the command returns `unavailable` with the reason — never a panic or a silent no-op
- [ ] Every subprocess goes through an injectable command runner (no shell strings; args as a list) so
      tests drive a fake `gh` and a real temp git repo
- [ ] Tests cover: clean tree, dirty tree, staged file, new branch, linked worktree, commit, diff body,
      absent binary, large diff truncation marker
- [ ] Git panel: status list, current branch, diff viewer with Monaco lazy-loaded, commit form with path
      selection, PR button that surfaces the URL or the unavailable reason
- [ ] Output is bounded and streamed/truncated with a clear marker; no command writes outside the repo
- [ ] Contract fixtures for `GitStatus`, `GitBranch`, `GitWorktree`, `GitDiff`, `PrResult`
