import type { GitFileStatus } from "@grove/contract";

export interface FileStat {
  added: number;
  removed: number;
}

/**
 * Per-file counts and the checkout total, read straight from one status payload.
 *
 * The counts travel with the status: git is asked once for the whole checkout
 * (`git diff --numstat` plus a local read of untracked files), never once per file.
 */
export function statsFromFiles(files: GitFileStatus[]): {
  fileStats: Record<string, FileStat>;
  total: FileStat;
} {
  const fileStats: Record<string, FileStat> = {};
  const total: FileStat = { added: 0, removed: 0 };
  for (const file of files) {
    fileStats[file.path] = { added: file.added, removed: file.removed };
    total.added += file.added;
    total.removed += file.removed;
  }
  return { fileStats, total };
}
