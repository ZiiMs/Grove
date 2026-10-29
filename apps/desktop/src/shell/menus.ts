import type { GitWorktree, Project, SessionSummary } from "@grove/contract";
import type { GroveBridge } from "../ipc/bridge";
import { errorText } from "../ipc/errors";
import type { MenuItem } from "../state/menuStore";
import { useSessionStore } from "../state/sessionStore";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore } from "../state/workspaceStore";
import { copyText } from "./clipboard";
import { revealPath } from "./reveal";

/**
 * The shell's context-menu items.
 *
 * `App.tsx` only mounts the tree and forwards an anchor; the actions and their confirmations live
 * here, so the shell's shared seam stays one line per callback. Every action reads the store at
 * selection time rather than closing over a render's state: a menu can stay open while an event
 * changes the row it was built from.
 */

async function copyWithToast(text: string, message: string): Promise<void> {
  const notify = useUiStore.getState().notify;
  if (await copyText(text)) notify(message);
  else notify("Clipboard is unavailable", "error");
}

/** Stop (after confirming a running session) and archive, then say what happened. */
export async function archiveSession(bridge: GroveBridge, row: SessionSummary): Promise<void> {
  const running = row.status !== "exited" && row.status !== "error";
  if (
    running &&
    !window.confirm(`Stop and archive "${row.title}"? The agent stops and the session moves to Archive.`)
  ) {
    return;
  }
  const archived = await useSessionStore.getState().archive(bridge, row.id);
  if (archived) useUiStore.getState().notify(`Archived ${row.title}`);
}

/**
 * Session menu. Generate name is the first entry on both sides of the archive: it overwrites any
 * title without confirmation, including a manual one, and is disabled while a generation for this
 * session is already running. Rename works on both sides too; permanent delete exists only on an
 * archived row, where Archive's action becomes Restore.
 */
export function sessionMenuItems(bridge: GroveBridge, row: SessionSummary): MenuItem[] {
  const generating = useSessionStore.getState().titleJobs.has(row.id);
  const items: MenuItem[] = [
    {
      kind: "item",
      label: generating ? "Generating…" : "Generate name with AI",
      icon: "sparkle",
      disabled: generating,
      onSelect: () => void useSessionStore.getState().generateTitle(bridge, row.id, true),
    },
    {
      kind: "item",
      label: "Rename",
      icon: "edit",
      onSelect: () => useUiStore.getState().startRename(row.id),
    },
  ];
  if (row.archivedAt) {
    items.push({
      kind: "item",
      label: "Restore",
      icon: "restore",
      onSelect: () => void useSessionStore.getState().restore(bridge, row.id),
    });
  } else {
    items.push({
      kind: "item",
      label: "Archive",
      icon: "archive",
      onSelect: () => void archiveSession(bridge, row),
    });
  }
  items.push(
    {
      kind: "item",
      label: "Copy working directory",
      icon: "copy",
      onSelect: () => void copyWithToast(row.cwd, "Working directory copied"),
    },
    {
      kind: "item",
      label: "Reveal in Explorer",
      icon: "folder",
      onSelect: () => void revealPath(bridge, row.cwd, useUiStore.getState().notify),
    },
  );
  if (row.archivedAt) {
    items.push(
      { kind: "separator" },
      {
        kind: "item",
        label: "Delete permanently",
        icon: "trash",
        danger: true,
        onSelect: () => {
          const confirmed = window.confirm(
            `Delete "${row.title}" permanently? Its transcript, plan, pins and subagents are removed. This cannot be undone.`,
          );
          if (!confirmed) return;
          void useSessionStore
            .getState()
            .deleteSession(bridge, row.id)
            .then((ok) => {
              if (ok) useUiStore.getState().notify(`Deleted ${row.title}`);
            });
        },
      },
    );
  }
  return items;
}

export function projectMenuItems(bridge: GroveBridge, project: Project): MenuItem[] {
  const notify = useUiStore.getState().notify;
  return [
    {
      kind: "item",
      label: "New session",
      icon: "plus",
      onSelect: () =>
        useUiStore.getState().openNewSession({ projectId: project.id, worktreePath: null }),
    },
    {
      kind: "item",
      label: "Copy path",
      icon: "copy",
      onSelect: () => void copyWithToast(project.path, "Path copied"),
    },
    {
      kind: "item",
      label: "Reveal in Explorer",
      icon: "folder",
      onSelect: () => void revealPath(bridge, project.path, notify),
    },
    {
      kind: "item",
      label: "Refresh",
      icon: "refresh",
      onSelect: () => void useWorkspaceStore.getState().refreshProjects(bridge),
    },
    { kind: "separator" },
    {
      kind: "item",
      label: "Remove project",
      icon: "trash",
      danger: true,
      onSelect: () => {
        const confirmed = window.confirm(
          `Remove ${project.name} from Grove? Its sessions become chats, and the folder is left on disk.`,
        );
        if (!confirmed) return;
        void (async () => {
          try {
            await useWorkspaceStore.getState().removeProject(bridge, project.id);
            await useSessionStore.getState().load(bridge);
            notify(`Removed ${project.name}`);
          } catch (error) {
            notify(errorText(error), "error");
          }
        })();
      },
    },
  ];
}

export function worktreeMenuItems(
  bridge: GroveBridge,
  project: Project,
  worktree: GitWorktree,
): MenuItem[] {
  const items: MenuItem[] = [
    {
      kind: "item",
      label: "New session",
      icon: "plus",
      onSelect: () =>
        useUiStore.getState().openNewSession({ projectId: project.id, worktreePath: worktree.path }),
    },
    {
      kind: "item",
      label: "Copy path",
      icon: "copy",
      onSelect: () => void copyWithToast(worktree.path, "Path copied"),
    },
    {
      kind: "item",
      label: "Reveal in Explorer",
      icon: "folder",
      onSelect: () => void revealPath(bridge, worktree.path, useUiStore.getState().notify),
    },
    {
      kind: "item",
      label: "Refresh",
      icon: "refresh",
      onSelect: () => void useWorkspaceStore.getState().refreshProjectGit(bridge, project),
    },
  ];
  // The main checkout is the repository itself; only linked worktrees can be deleted.
  if (!worktree.current) {
    items.push(
      { kind: "separator" },
      {
        kind: "item",
        label: "Delete worktree",
        icon: "trash",
        danger: true,
        onSelect: () => useUiStore.getState().openWorktreeRemoval(project, worktree),
      },
    );
  }
  return items;
}
