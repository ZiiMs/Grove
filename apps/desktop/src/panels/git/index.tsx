import type {
  GitBranch,
  GitDiff,
  GitStatus,
  GitWorktree,
  IpcError,
  PrResult,
} from "@grove/contract";
import { lazy, Suspense, useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import type { UnlistenFn } from "../../ipc/bridge";
import { useBridge } from "../../ipc/context";
import { subscribeToGitChanged } from "../../ipc/events";
import type { GitCommitArgs, GitDiffArgs, GitOpenPrArgs, GitStatusArgs } from "../../ipc/commands/git";
import { PanelFrame } from "../PanelFrame";
import "./git.css";

/**
 * Monaco is a feature chunk (architecture.md §Rendering): the dynamic import is only requested when a
 * diff is opened, never on panel mount.
 */
const DiffViewer = lazy(async () => {
  const { Editor } = await import("@monaco-editor/react");
  return { default: Editor };
});

/** Command rejections cross the bridge as plain `{ code, message, detail }` values, not `Error`s. */
function asIpcError(error: unknown): IpcError {
  if (typeof error === "object" && error !== null) {
    const candidate = error as { code?: unknown; message?: unknown; detail?: unknown };
    if (typeof candidate.code === "string" && typeof candidate.message === "string") {
      return {
        code: candidate.code as IpcError["code"],
        message: candidate.message,
        ...(typeof candidate.detail === "string" ? { detail: candidate.detail } : {}),
      };
    }
  }
  return { code: "io", message: error instanceof Error ? error.message : String(error) };
}

function shortHead(head: string | null): string {
  return head ? head.slice(0, 7) : "—";
}

export function GitPanel() {
  const bridge = useBridge();
  const [repoPath, setRepoPath] = useState("");
  const [status, setStatus] = useState<GitStatus | null>(null);
  const [branches, setBranches] = useState<GitBranch[]>([]);
  const [worktrees, setWorktrees] = useState<GitWorktree[]>([]);
  const [diff, setDiff] = useState<GitDiff | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [message, setMessage] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const [prTitle, setPrTitle] = useState("");
  const [prBody, setPrBody] = useState("");
  const [prBase, setPrBase] = useState("");
  const [pr, setPr] = useState<PrResult | null>(null);
  const [error, setError] = useState<IpcError | null>(null);
  const [loading, setLoading] = useState(true);

  const locationArgs = useMemo<GitStatusArgs>(() => {
    const path = repoPath.trim();
    return path ? { repoPath: path } : {};
  }, [repoPath]);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [nextStatus, nextBranches, nextWorktrees] = await Promise.all([
        bridge.invoke("git_status", locationArgs),
        bridge.invoke("git_branches", locationArgs),
        bridge.invoke("git_worktrees", locationArgs),
      ]);
      setStatus(nextStatus);
      setBranches(nextBranches);
      setWorktrees(nextWorktrees);
      setSelected([]);
    } catch (err) {
      setStatus(null);
      setError(asIpcError(err));
    } finally {
      setLoading(false);
    }
  }, [bridge, locationArgs]);

  useEffect(() => {
    void load();
  }, [load]);

  // A commit made anywhere in the app publishes the fresh status; adopt it without another call.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;
    void subscribeToGitChanged(bridge, (next) => {
      setStatus(next);
      setSelected([]);
    }).then((stop) => {
      if (cancelled) stop();
      else unlisten = stop;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [bridge]);

  async function openDiff(path: string) {
    setError(null);
    try {
      const args: GitDiffArgs = { ...locationArgs, path };
      setDiff(await bridge.invoke("git_diff", args));
    } catch (err) {
      setError(asIpcError(err));
    }
  }

  function togglePath(path: string) {
    setSelected((current) =>
      current.includes(path) ? current.filter((entry) => entry !== path) : [...current, path],
    );
  }

  async function submitCommit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!message.trim() || selected.length === 0) return;
    setError(null);
    setNote(null);
    try {
      const args: GitCommitArgs = { ...locationArgs, message: message.trim(), paths: selected };
      const next = await bridge.invoke("git_commit", args);
      setStatus(next);
      setSelected([]);
      setMessage("");
      setNote(`Committed ${shortHead(next.head)} on ${next.branch ?? "detached HEAD"}`);
      setBranches(await bridge.invoke("git_branches", locationArgs));
    } catch (err) {
      setError(asIpcError(err));
    }
  }

  async function openPullRequest(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!prTitle.trim()) return;
    setError(null);
    setPr(null);
    try {
      const args: GitOpenPrArgs = { ...locationArgs, title: prTitle.trim(), body: prBody };
      if (prBase.trim()) args.base = prBase.trim();
      setPr(await bridge.invoke("git_open_pr", args));
    } catch (err) {
      setError(asIpcError(err));
    }
  }

  return (
    <PanelFrame
      title="Git"
      subtitle={
        status
          ? `${status.repoRoot} · ${status.branch ?? "detached HEAD"}`
          : "No repository loaded"
      }
      actions={
        <form
          className="git-repo-form"
          onSubmit={(event) => {
            event.preventDefault();
            void load();
          }}
        >
          <input
            className="input"
            aria-label="Repository path"
            placeholder="Repository path (defaults to the app's working directory)"
            value={repoPath}
            onChange={(event) => setRepoPath(event.target.value)}
          />
          <button className="input" type="submit" disabled={loading}>
            {loading ? "Loading…" : "Load"}
          </button>
        </form>
      }
    >
      {error ? (
        <div className="git-error" role="alert">
          <strong>{error.code}</strong> {error.message}
          {error.detail ? <p className="git-detail">{error.detail}</p> : null}
        </div>
      ) : null}
      {note ? (
        <p className="git-note" role="status">
          {note}
        </p>
      ) : null}
      {!status && !error ? <p className="git-empty">Loading repository…</p> : null}

      {status ? (
        <div className="git-grid">
          <section className="git-section" aria-label="Status">
            <h2>Status</h2>
            <p>
              Current branch <code>{status.branch ?? "detached HEAD"}</code> at{" "}
              <code>{shortHead(status.head)}</code> ·{" "}
              {status.clean ? "clean" : `${status.files.length} changed`}
              {status.truncated ? " · output truncated" : ""}
            </p>
            {status.files.length === 0 ? (
              <p className="git-empty">Nothing to commit.</p>
            ) : (
              <ul className="git-file-list">
                {status.files.map((file) => (
                  <li key={file.path}>
                    <label>
                      <input
                        type="checkbox"
                        aria-label={`Select ${file.path}`}
                        checked={selected.includes(file.path)}
                        onChange={() => togglePath(file.path)}
                      />
                      <code className="git-code">
                        {file.index}
                        {file.worktree}
                      </code>{" "}
                      {file.from ? `${file.from} → ` : ""}
                      {file.path}
                    </label>
                    <button
                      type="button"
                      className="input"
                      aria-label={`View diff for ${file.path}`}
                      onClick={() => void openDiff(file.path)}
                    >
                      Diff
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section className="git-section" aria-label="Branches">
            <h2>Branches</h2>
            <ul className="git-branch-list">
              {branches.map((branch) => (
                <li key={branch.name} className={branch.current ? "is-current" : undefined}>
                  <code>{branch.name}</code>
                  {branch.current ? " (current)" : ""}
                  {branch.remote ? ` · ${branch.remote}` : ""}
                  {branch.ahead > 0 ? ` · ↑${branch.ahead}` : ""}
                  {branch.behind > 0 ? ` · ↓${branch.behind}` : ""}
                </li>
              ))}
            </ul>
          </section>

          <section className="git-section" aria-label="Worktrees">
            <h2>Worktrees</h2>
            <ul className="git-worktree-list">
              {worktrees.map((worktree) => (
                <li key={worktree.path} className={worktree.current ? "is-current" : undefined}>
                  <code>{worktree.branch ?? (worktree.detached ? "detached HEAD" : "bare")}</code>{" "}
                  · {worktree.path}
                  {worktree.current ? " (current)" : ""}
                </li>
              ))}
            </ul>
          </section>

          <section className="git-section git-section-diff" aria-label="Diff">
            <h2>Diff</h2>
            {diff ? (
              <>
                <p className="git-diff-meta">
                  {diff.target}
                  {diff.path ? ` · ${diff.path}` : ""}
                  {diff.truncated ? " · truncated" : ""}
                </p>
                <Suspense fallback={<p className="git-empty">Loading diff viewer…</p>}>
                  <DiffViewer
                    height="360px"
                    language="diff"
                    value={diff.patch}
                    theme="vs-dark"
                    options={{ readOnly: true, minimap: { enabled: false }, scrollBeyondLastLine: false }}
                  />
                </Suspense>
              </>
            ) : (
              <p className="git-empty">Choose a changed file to view its diff.</p>
            )}
          </section>

          <section className="git-section" aria-label="Commit">
            <h2>Commit</h2>
            <form className="git-form" aria-label="Commit form" onSubmit={(event) => void submitCommit(event)}>
              <label>
                Message
                <input
                  className="input"
                  aria-label="Commit message"
                  value={message}
                  onChange={(event) => setMessage(event.target.value)}
                />
              </label>
              <p className="git-empty">
                {selected.length === 0 ? "Select paths above." : `${selected.length} path(s) selected`}
              </p>
              <button
                className="input primary"
                type="submit"
                disabled={!message.trim() || selected.length === 0}
              >
                Commit selected paths
              </button>
            </form>
          </section>

          <section className="git-section" aria-label="Pull request">
            <h2>Draft pull request</h2>
            <form
              className="git-form"
              aria-label="Draft pull request form"
              onSubmit={(event) => void openPullRequest(event)}
            >
              <label>
                Title
                <input
                  className="input"
                  aria-label="PR title"
                  value={prTitle}
                  onChange={(event) => setPrTitle(event.target.value)}
                />
              </label>
              <label>
                Body
                <textarea
                  className="input"
                  aria-label="PR body"
                  value={prBody}
                  onChange={(event) => setPrBody(event.target.value)}
                />
              </label>
              <label>
                Base
                <input
                  className="input"
                  aria-label="PR base"
                  placeholder="main"
                  value={prBase}
                  onChange={(event) => setPrBase(event.target.value)}
                />
              </label>
              <button className="input primary" type="submit" disabled={!prTitle.trim()}>
                Open draft PR
              </button>
            </form>
            {pr ? (
              <p className="git-note" role="status">
                Draft PR: <a href={pr.url}>{pr.url}</a>
              </p>
            ) : null}
          </section>
        </div>
      ) : null}
    </PanelFrame>
  );
}
