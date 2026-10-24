import { z } from "zod";

/**
 * Git panel contract: status, branches, worktrees, diffs, and draft pull requests.
 *
 * Rust (`grove_core::gitops`) owns the wire shape; these schemas must accept exactly what it serialises.
 * Both sides are pinned by `contract/fixtures/git-*.json` — a Rust test serialises a sample per DTO and
 * this package parses the same file with the schema below.
 */

export const gitFileStatusSchema = z.object({
  /** Repository-relative path, forward slashes. */
  path: z.string(),
  /** Previous path for a rename or copy. */
  from: z.string().nullable(),
  /** Index (staged) status character, e.g. `"M"`, `"A"`, `"?"`. */
  index: z.string(),
  /** Worktree (unstaged) status character, e.g. `"M"`, `" "`, `"?"`. */
  worktree: z.string(),
  staged: z.boolean(),
  untracked: z.boolean(),
});
export type GitFileStatus = z.infer<typeof gitFileStatusSchema>;

export const gitStatusSchema = z.object({
  repoRoot: z.string(),
  /** Current branch; `null` on a detached HEAD. */
  branch: z.string().nullable(),
  /** Full commit id of HEAD; `null` before the first commit. */
  head: z.string().nullable(),
  clean: z.boolean(),
  truncated: z.boolean(),
  files: z.array(gitFileStatusSchema),
});
export type GitStatus = z.infer<typeof gitStatusSchema>;

export const gitBranchSchema = z.object({
  /** Local name (`main`) or remote-tracking name (`origin/main`). */
  name: z.string(),
  /** Remote name for a remote-tracking branch, `null` for a local branch. */
  remote: z.string().nullable(),
  current: z.boolean(),
  upstream: z.string().nullable(),
  ahead: z.number().int().nonnegative(),
  behind: z.number().int().nonnegative(),
});
export type GitBranch = z.infer<typeof gitBranchSchema>;

export const gitWorktreeSchema = z.object({
  path: z.string(),
  head: z.string().nullable(),
  branch: z.string().nullable(),
  bare: z.boolean(),
  detached: z.boolean(),
  current: z.boolean(),
});
export type GitWorktree = z.infer<typeof gitWorktreeSchema>;

export const diffTargetSchema = z.enum(["working_tree", "staged", "branch"]);
export type DiffTarget = z.infer<typeof diffTargetSchema>;

export const gitDiffSchema = z.object({
  repoRoot: z.string(),
  target: diffTargetSchema,
  /** Restrict the diff to one path, or `null` for everything. */
  path: z.string().nullable(),
  base: z.string().nullable(),
  branch: z.string().nullable(),
  /** Unified diff text; when `truncated` it ends with a marker line. */
  patch: z.string(),
  truncated: z.boolean(),
});
export type GitDiff = z.infer<typeof gitDiffSchema>;

export const prResultSchema = z.object({
  url: z.string(),
  draft: z.boolean(),
  base: z.string().nullable(),
});
export type PrResult = z.infer<typeof prResultSchema>;

/** `grove://git-changed` carries the fresh status payload, so the panel updates without a re-fetch. */
export const gitChangedSchema = z.object({
  sessionId: z.string().optional(),
  payload: gitStatusSchema,
});
export type GitChangedEvent = z.infer<typeof gitChangedSchema>;

/** Validate an incoming `grove://git-changed` payload; malformed data is dropped, not thrown. */
export function parseGitChanged(raw: unknown): GitChangedEvent | null {
  const parsed = gitChangedSchema.safeParse(raw);
  return parsed.success ? parsed.data : null;
}
