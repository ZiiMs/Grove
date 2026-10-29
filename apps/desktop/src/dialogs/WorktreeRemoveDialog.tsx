import type { GitStatus } from "@grove/contract";
import { useEffect, useState } from "react";
import { useBridge } from "../ipc/context";
import { errorText } from "../ipc/errors";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore } from "../state/workspaceStore";
import { Icon } from "../workspace/Icon";
import { Dialog } from "./Dialog";

/**
 * Remove a linked worktree. Every destructive extra is opt-in and the core preflights them before
 * touching the folder, so an unmerged branch or a dirty checkout cannot be half-deleted.
 */
export function WorktreeRemoveDialog() {
  const bridge = useBridge();
  const removal = useUiStore((state) => state.worktreeRemoval);
  const close = useUiStore((state) => state.closeWorktreeRemoval);
  const notify = useUiStore((state) => state.notify);
  const refreshProjectGit = useWorkspaceStore((state) => state.refreshProjectGit);
  const [deleteBranch, setDeleteBranch] = useState(false);
  const [forceBranch, setForceBranch] = useState(false);
  const [force, setForce] = useState(false);
  const [dirty, setDirty] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [working, setWorking] = useState(false);

  useEffect(() => {
    if (!removal) return;
    setDeleteBranch(false);
    setForceBranch(false);
    setForce(false);
    setDirty(null);
    setError(null);
    setWorking(false);
    let active = true;
    void bridge
      .invoke("git_status", { repoPath: removal.worktree.path })
      .then((status: GitStatus) => {
        if (active) setDirty(status.files.length);
      })
      .catch(() => {
        // The core still preflights on submit; a failed read only costs the warning.
        if (active) setDirty(null);
      });
    return () => {
      active = false;
    };
  }, [bridge, removal]);

  if (!removal) return null;

  const branch = removal.worktree.branch;
  const changed = dirty ?? 0;
  const blockedByDirty = dirty !== null && changed > 0 && !force;

  const submit = async () => {
    setWorking(true);
    setError(null);
    try {
      await bridge.invoke("git_worktree_remove", {
        repoPath: removal.project.path,
        worktreePath: removal.worktree.path,
        deleteBranch,
        force,
        forceBranch: deleteBranch && forceBranch,
      });
      await refreshProjectGit(bridge, removal.project);
      notify(`Deleted worktree ${branch ?? removal.worktree.path}`);
      close();
    } catch (err) {
      setError(errorText(err));
    } finally {
      setWorking(false);
    }
  };

  return (
    <Dialog
      open
      className="worktree-delete-dialog"
      labelledBy="wt-delete-title"
      onClose={close}
    >
      <div className="dlg-head">
        <h2 id="wt-delete-title">Delete worktree</h2>
        <button className="icon-btn sm" type="button" aria-label="Close dialog" onClick={close}>
          <Icon name="x" size="sm" />
        </button>
      </div>
      <p className="dlg-sub">
        This removes the checkout folder from disk. Sessions that ran there keep their transcripts,
        but their files are gone.
      </p>

      <p className="path-preview">{removal.worktree.path}</p>
      <p className="field-hint">
        Branch: <code>{branch ?? "detached HEAD"}</code>
      </p>
      {dirty === null ? null : dirty > 0 ? (
        <p className="field-err" role="alert">
          {dirty} uncommitted file{dirty === 1 ? "" : "s"} in this worktree. Tick “Discard uncommitted
          changes” to remove it anyway.
        </p>
      ) : (
        <p className="field-hint">The worktree is clean.</p>
      )}

      <div className="delete-options">
        <label className="delete-opt">
          <input
            type="checkbox"
            checked={deleteBranch}
            disabled={!branch}
            onChange={(event) => {
              setDeleteBranch(event.target.checked);
              if (!event.target.checked) setForceBranch(false);
            }}
          />
          <span>
            <strong>Delete the local branch</strong>
            <span className="opt">
              {branch ? `Remove ${branch} after the folder.` : "Detached worktree: no branch to delete."}
            </span>
          </span>
        </label>
        <label className="delete-opt" data-disabled={deleteBranch ? undefined : "true"}>
          <input
            type="checkbox"
            checked={forceBranch}
            disabled={!deleteBranch}
            onChange={(event) => setForceBranch(event.target.checked)}
          />
          <span>
            <strong>Delete it even if it is unmerged</strong>
            <span className="opt">Discards commits that are not in the main checkout.</span>
          </span>
        </label>
        <label className="delete-opt">
          <input
            type="checkbox"
            checked={force}
            onChange={(event) => setForce(event.target.checked)}
          />
          <span>
            <strong>Discard uncommitted changes</strong>
            <span className="opt">Remove the checkout even when it has local edits.</span>
          </span>
        </label>
      </div>

      {error ? (
        <p className="field-err" role="alert">
          {error}
        </p>
      ) : null}

      <div className="dlg-foot">
        <button className="btn-ghost" type="button" disabled={working} onClick={close}>
          Cancel
        </button>
        <button
          className="btn-primary danger"
          type="button"
          disabled={working || blockedByDirty}
          onClick={() => void submit()}
        >
          {working ? "Deleting…" : "Delete worktree"}
        </button>
      </div>
    </Dialog>
  );
}
