import type { Project, SessionSummary } from "@grove/contract";

export function normalizePath(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
}

/**
 * Normalize a folder path the user typed or pasted: trim whitespace and a matching pair of
 * surrounding quotes, use forward slashes, collapse repeated separators, and keep a root's trailing
 * slash. Mirrors `grove_core::projects::normalize_path`; `~` is left for the core to expand.
 */
export function normalizeFolderPath(path: string): string {
  let value = path.trim();
  if (value.length >= 2) {
    const first = value[0];
    const last = value[value.length - 1];
    if ((first === '"' && last === '"') || (first === "'" && last === "'")) {
      value = value.slice(1, -1).trim();
    }
  }
  if (!value) return "";

  const unified = value.replace(/\\/g, "/");
  const unc = unified.startsWith("//");
  const absolute = unified.startsWith("/");
  const parts = unified.split("/").filter((part) => part.length > 0);
  if (parts.length === 0) return unc ? "//" : absolute ? "/" : "";
  if (/^[A-Za-z]:$/.test(parts[0])) {
    return parts.length === 1 ? `${parts[0]}/` : `${parts[0]}/${parts.slice(1).join("/")}`;
  }
  return `${unc ? "//" : absolute ? "/" : ""}${parts.join("/")}`;
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
