import type {
  GitBranch,
  GitDiff,
  GitStatus,
  GitWorktree,
  IpcError,
  PrResult,
  SpecDoc,
} from "@grove/contract";
import MarkdownIt from "markdown-it";
import { lazy, Suspense, useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import type { GroveBridge, UnlistenFn } from "../../ipc/bridge";
import { useBridge } from "../../ipc/context";
import { toIpcError } from "../../ipc/errors";
import { subscribeToGitChanged } from "../../ipc/events";
import type { GitCommitArgs, GitDiffArgs, GitOpenPrArgs, GitStatusArgs } from "../../ipc/commands/git";
import { useSessionStore } from "../../state/sessionStore";
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

/** Spec bodies are rendered, not executed: HTML in a spec is text, exactly as the Specs panel does it. */
const markdown = new MarkdownIt({ html: false, linkify: false, typographer: false });

function shortHead(head: string | null): string {
  return head ? head.slice(0, 7) : "—";
}

/**
 * The other half of the review loop: the spec the change was meant to satisfy, beside the diff.
 *
 * Titles come from the active session's context (`session_context_get`), which is the same snapshot the
 * Sessions header shows, so the two can never disagree about what is pinned. The body is read with
 * `spec_read` — the graph owns where a spec id lives — and rendered with HTML disabled, because a spec
 * is text and Grove renders rather than executes.
 *
 * It is a pane rather than a second panel because a session can only be watched from one panel at a
 * time; this is how the goal spec's "see the spec it was meant to satisfy beside the diff" is reachable.
 */
function SpecBesideDiff({ bridge }: { bridge: GroveBridge }) {
  const activeId = useSessionStore((state) => state.activeId);
  const [pins, setPins] = useState<Array<{ specId: string; title: string }>>([]);
  const [paths, setPaths] = useState<Record<string, string>>({});
  const [selected, setSelected] = useState<string | null>(null);
  const [doc, setDoc] = useState<SpecDoc | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState(true);

  useEffect(() => {
    if (!activeId) {
      setPins([]);
      setSelected(null);
      setDoc(null);
      return;
    }
    let cancelled = false;
    setError(null);
    void (async () => {
      try {
        const context = await bridge.invoke("session_context_get", { id: activeId });
        if (cancelled) return;
        setPins(context.pins);
        setSelected(context.pins[0]?.specId ?? null);
        if (context.pins.length === 0) return;
        // A pin stores the spec id; the graph knows which file that is.
        const graph = await bridge.invoke("spec_index");
        if (cancelled) return;
        setPaths(Object.fromEntries(graph.nodes.map((node) => [node.id, node.path])));
      } catch (err) {
        if (!cancelled) setError(toIpcError(err).message);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [bridge, activeId]);

  const path = selected ? (paths[selected] ?? null) : null;

  useEffect(() => {
    if (!path) {
      setDoc(null);
      return;
    }
    let cancelled = false;
    bridge
      .invoke("spec_read", { path })
      .then((next) => {
        if (!cancelled) setDoc(next);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(toIpcError(err).message);
      });
    return () => {
      cancelled = true;
    };
  }, [bridge, path]);

  const rendered = useMemo(() => (doc ? markdown.render(doc.body) : ""), [doc]);

  if (!activeId) return null;

  return (
    <aside className="git-spec-pane" aria-label="Spec beside the diff" data-testid="git-spec-pane">
      <header className="git-spec-head">
        <h2>Spec beside the diff</h2>
        <button
          type="button"
          className="input"
          data-testid="git-spec-toggle"
          aria-expanded={open}
          onClick={() => setOpen((value) => !value)}
        >
          {open ? "Hide" : "Show"}
        </button>
      </header>
      {open ? (
        <>
          {error ? (
            <p className="git-empty" role="alert">
              {error}
            </p>
          ) : null}
          {pins.length === 0 && !error ? (
            <p className="git-empty" data-testid="git-spec-empty">
              No specs pinned to this session. Pin one from the Specs panel to read it here.
            </p>
          ) : null}
          {pins.length > 0 ? (
            <ul className="git-spec-pins" aria-label="Pinned specs">
              {pins.map((pin) => (
                <li key={pin.specId}>
                  <button
                    type="button"
                    className={pin.specId === selected ? "git-spec-pin is-active" : "git-spec-pin"}
                    aria-current={pin.specId === selected ? "true" : undefined}
                    onClick={() => setSelected(pin.specId)}
                  >
                    {pin.title}
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
          {doc ? (
            <article
              className="git-spec-body"
              data-testid="git-spec-body"
              // markdown-it runs with `html: false`, so a `<script>` in a spec is text.
              dangerouslySetInnerHTML={{ __html: rendered }}
            />
          ) : null}
        </>
      ) : null}
    </aside>
  );
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
      setError(toIpcError(err));
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
      setError(toIpcError(err));
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
      setError(toIpcError(err));
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
      setError(toIpcError(err));
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

          <section className="git-section git-section-diff" aria-label="Diff" data-testid="git-diff-section">
            <h2>Diff</h2>
            <div className="git-diff-row">
              <div className="git-diff-main">
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
                        options={{
                          readOnly: true,
                          minimap: { enabled: false },
                          scrollBeyondLastLine: false,
                        }}
                      />
                    </Suspense>
                  </>
                ) : (
                  <p className="git-empty">Choose a changed file to view its diff.</p>
                )}
              </div>
              <SpecBesideDiff bridge={bridge} />
            </div>
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
