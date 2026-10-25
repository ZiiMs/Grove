import { useMemo, useState } from "react";
import type { GitFileStatus, Project, SessionDetail } from "@grove/contract";
import { useBridge } from "../ipc/context";
import { errorText } from "../ipc/errors";
import { useContextStore } from "../state/contextStore";
import { useUiStore } from "../state/uiStore";
import { Icon } from "../workspace/Icon";
import { basename, dirname } from "../workspace/tree";

interface GitPanelProps {
  session: SessionDetail | null;
  project: Project | null;
}

function statusFlag(file: GitFileStatus): string {
  if (file.untracked) return "U";
  const raw = file.staged ? file.index : file.worktree;
  if (raw === " " || raw === "?") return "M";
  return raw;
}

/** The Git tab: branch header, staged changes, and the commit composer. */
export function GitPanel({ session, project }: GitPanelProps) {
  const bridge = useBridge();
  const git = useContextStore((state) => state.git);
  const gitError = useContextStore((state) => state.gitError);
  const activePath = useContextStore((state) => state.activePath);
  const refreshGit = useContextStore((state) => state.refreshGit);
  const stage = useContextStore((state) => state.stage);
  const commit = useContextStore((state) => state.commit);
  const fileStats = useContextStore((state) => state.fileStats);
  const openDiff = useUiStore((state) => state.openDiff);
  const openPr = useUiStore((state) => state.openPr);
  const notify = useUiStore((state) => state.notify);

  const [summary, setSummary] = useState("");
  const [description, setDescription] = useState("");
  const [working, setWorking] = useState(false);
  const [generating, setGenerating] = useState(false);

  const files = useMemo(() => git?.files ?? [], [git]);
  const staged = files.filter((file) => file.staged);
  const stagedCount = staged.length;
  const allStaged = files.length > 0 && stagedCount === files.length;
  const branch = git?.branch ?? null;
  const isGitProject = project?.isGit ?? false;

  if (!session) {
    return (
      <div className="insp-pane">
        <div className="pane-head">
          <span className="ph-mono">No session</span>
        </div>
        <div className="git-empty">
          <strong>No working tree</strong>
          Select a session to see the checkout it runs in.
        </div>
      </div>
    );
  }

  if (!project) {
    return (
      <div className="insp-pane">
        <div className="pane-head">
          <span className="ph-mono">Chats</span>
          <span className="ph-tail">no worktree</span>
        </div>
        <div className="git-empty">
          <strong>No working tree</strong>
          Chats run outside a repository, so there is nothing to commit.
        </div>
      </div>
    );
  }

  if (!isGitProject) {
    return (
      <div className="insp-pane">
        <div className="pane-head">
          <span className="ph-mono">{project.name}</span>
          <span className="ph-tail">no repository</span>
        </div>
        <div className="git-empty">
          <strong>Not a Git repository</strong>
          {project.name} was added as a plain folder, so there is nothing to stage or commit.
        </div>
      </div>
    );
  }

  const toggleStage = async (file: GitFileStatus) => {
    await stage(bridge, [file.path], !file.staged);
  };

  const toggleAll = async () => {
    if (files.length === 0) return;
    await stage(
      bridge,
      files.map((file) => file.path),
      !allStaged,
    );
  };

  const generate = () => {
    if (stagedCount === 0) {
      notify("Stage a file first — the description is written from the staged diff", "error");
      return;
    }
    setGenerating(true);
    window.setTimeout(() => {
      const names = staged.map((file) => basename(file.path));
      const list =
        names.slice(0, 2).join(" and ") +
        (names.length > 2 ? ` and ${names.length - 2} more` : "");
      const first = staged[0];
      const verb = first.untracked ? "Add" : statusFlag(first) === "D" ? "Remove" : "Update";
      const text = `${verb} ${list} on ${(branch ?? "the working tree").split("/").pop()?.replace(/-/g, " ")}`;
      setSummary(text.length > 72 ? `${text.slice(0, 69)}…` : text);
      setDescription(
        `Staged on ${branch ?? "the working tree"}:\n` +
          staged
            .map((file) => {
              const stat = fileStats[file.path] ?? { added: 0, removed: 0 };
              return `- ${basename(file.path)} (${dirname(file.path) || "workspace root"}): +${stat.added} −${stat.removed}`;
            })
            .join("\n"),
      );
      setGenerating(false);
    }, 450);
  };

  const doCommit = async (andPr: boolean) => {
    if (!summary.trim() || stagedCount === 0 || working) return;
    setWorking(true);
    try {
      await commit(
        bridge,
        summary.trim(),
        staged.map((file) => file.path),
      );
      const seed = { title: summary.trim(), body: description };
      setSummary("");
      setDescription("");
      notify(
        `${stagedCount} file${stagedCount === 1 ? "" : "s"} committed on ${branch ?? "the branch"}`,
      );
      await refreshGit(bridge, activePath);
      if (andPr) openPr(seed);
    } catch (error) {
      notify(errorText(error), "error");
    } finally {
      setWorking(false);
    }
  };

  const empty = !git;
  const hint = empty
    ? gitError ?? "No changes to read."
    : files.length === 0
      ? "Nothing to commit — the working tree is clean."
      : stagedCount === 0
        ? "Stage at least one file to commit."
        : `${stagedCount} file${stagedCount === 1 ? "" : "s"} ready to commit.`;

  return (
    <div className="insp-pane">
      <div className="pane-head">
        <span className="ph-mono">{branch ?? "detached HEAD"}</span>
        <span className="ph-tail">
          {files.length} file{files.length === 1 ? "" : "s"}
        </span>
      </div>

      <div className="git-changes-head">
        <span className="gch-title">Changes</span>
        <span className="gch-count">
          {files.length ? `${stagedCount} of ${files.length} staged` : "nothing to stage"}
        </span>
        {files.length ? (
          <button className="gch-btn" type="button" onClick={() => void toggleAll()}>
            {allStaged ? "Unstage all" : "Stage all"}
          </button>
        ) : null}
      </div>

      <div className="git-changes od-scroll">
        {files.length === 0 ? (
          <div className="git-empty">
            <strong>{empty ? (gitError ? "Git is unavailable" : "No repository") : "Working tree clean"}</strong>
            {empty
              ? gitError ?? "This session has no git checkout."
              : `No changes in ${branch ?? "this checkout"}. Files this session edits will appear here.`}
          </div>
        ) : (
          files.map((file) => {
            const stat = fileStats[file.path] ?? { added: 0, removed: 0 };
            return (
              <div className="gc-row" key={file.path} data-staged={file.staged}>
                <button
                  className="gc-stage"
                  type="button"
                  role="checkbox"
                  aria-checked={file.staged}
                  aria-label={`${file.staged ? "Unstage " : "Stage "}${basename(file.path)}`}
                  onClick={() => void toggleStage(file)}
                >
                  {file.staged ? <Icon name="check" size="xs" /> : null}
                </button>
                <button
                  className="gc-body"
                  type="button"
                  style={{ textAlign: "left", minWidth: 0, background: "none" }}
                  onClick={() => openDiff(file.path)}
                >
                  <span className="gc-name">{basename(file.path)}</span>
                  <span className="gc-dir">{dirname(file.path) || "workspace root"}</span>
                </button>
                <span className="gc-stat">
                  <span className="add">+{stat.added}</span>
                  <span className="del">−{stat.removed}</span>
                </span>
                <button
                  className="gc-diffbtn"
                  type="button"
                  aria-label={`View diff for ${basename(file.path)}`}
                  title="View diff"
                  onClick={() => openDiff(file.path)}
                >
                  <Icon name="layers" size="xs" />
                </button>
              </div>
            );
          })
        )}
      </div>

      <div className="commit-box">
        <div className="cb-top">
          <label className="cb-label" htmlFor="commit-summary">
            Commit summary
          </label>
          <button
            className="cb-gen"
            type="button"
            title="Generate a description from the staged diff"
            disabled={generating}
            onClick={generate}
          >
            <Icon name="sparkle" size="sm" />
            {generating ? "Writing…" : "Generate"}
          </button>
        </div>
        <input
          id="commit-summary"
          type="text"
          placeholder="What changed?"
          autoComplete="off"
          value={summary}
          onChange={(event) => setSummary(event.target.value)}
        />
        <textarea
          rows={2}
          placeholder="Extended description (optional)"
          value={description}
          onChange={(event) => setDescription(event.target.value)}
        />
        <p className="cb-hint">{hint}</p>
        <div className="cb-actions">
          <button
            className="btn-ghost sm"
            type="button"
            disabled={!summary.trim() || stagedCount === 0 || working}
            onClick={() => void doCommit(false)}
          >
            Commit
          </button>
          <button
            className="btn-primary cb-pr"
            type="button"
            disabled={!summary.trim() || stagedCount === 0 || working}
            onClick={() => void doCommit(true)}
          >
            <Icon name="branch" size="sm" />
            Commit &amp; open PR
          </button>
        </div>
      </div>
    </div>
  );
}
