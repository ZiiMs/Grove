import type { Project, SessionSummary } from "@grove/contract";

export function normalizePath(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
}

export function projectForSession(
  session: SessionSummary,
  projects: Project[],
): Project | null {
  if (session.projectId) {
    return projects.find((project) => project.id === session.projectId) ?? null;
  }
  const cwd = normalizePath(session.cwd);
  return (
    projects.find((project) => {
      const path = normalizePath(project.path);
      return cwd === path || cwd.startsWith(`${path}/`);
    }) ?? null
  );
}

/** The worktree a session runs in, by path prefix; sessions can sit in a subfolder. */
export function worktreeForSession<T extends { path: string }>(
  session: SessionSummary,
  worktrees: T[],
): T | null {
  const cwd = normalizePath(session.cwd);
  let best: T | null = null;
  let bestLength = -1;
  for (const worktree of worktrees) {
    const path = normalizePath(worktree.path);
    if ((cwd === path || cwd.startsWith(`${path}/`)) && path.length > bestLength) {
      best = worktree;
      bestLength = path.length;
    }
  }
  return best;
}

export function basename(path: string): string {
  const clean = path.replace(/\\/g, "/").replace(/\/+$/, "");
  return clean.split("/").pop() || clean;
}

export function dirname(path: string): string {
  const clean = path.replace(/\\/g, "/");
  const index = clean.lastIndexOf("/");
  return index === -1 ? "" : clean.slice(0, index);
}
