import type { GitBranch, GitDiff, GitStatus, GitWorktree, PrResult } from "@grove/contract";

/**
 * Git panel command surface. Argument names match the Rust command parameters (Tauri passes them as
 * camelCase); every command takes an optional repository path and defaults to the core's cwd.
 */
export interface GitStatusArgs {
  repoPath?: string;
}

export interface GitDiffArgs {
  repoPath?: string;
  /** Restrict the diff to one repository-relative path. */
  path?: string;
  /** Diff this branch against `base` (`base...branch`); both must be set. */
  branch?: string;
  base?: string;
  /** Diff the index instead of the working tree. */
  staged?: boolean;
}

export interface GitCommitArgs {
  repoPath?: string;
  message: string;
  paths: string[];
}

export interface GitOpenPrArgs {
  repoPath?: string;
  title: string;
  body: string;
  base?: string;
  head?: string;
}

export interface GitCommands {
  git_status: { args: GitStatusArgs; result: GitStatus };
  git_branches: { args: GitStatusArgs; result: GitBranch[] };
  git_worktrees: { args: GitStatusArgs; result: GitWorktree[] };
  git_diff: { args: GitDiffArgs; result: GitDiff };
  git_commit: { args: GitCommitArgs; result: GitStatus };
  git_open_pr: { args: GitOpenPrArgs; result: PrResult };
}
