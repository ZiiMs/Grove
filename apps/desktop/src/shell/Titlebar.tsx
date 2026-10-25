import type { GitBranch, GitStatus, Project, SessionDetail } from "@grove/contract";
import { BrandMark, Icon } from "../workspace/Icon";

interface TitlebarProps {
  project: Project | null;
  session: SessionDetail | null;
  git: GitStatus | null;
  gitStats: { added: number; removed: number } | null;
  branches: GitBranch[];
  awaitingCount: number;
  onGit: () => void;
  onAwaiting: () => void;
  onNewSession: () => void;
}

/** Brand, the active checkout chip, and the two window-level actions. */
export function Titlebar({
  project,
  session,
  git,
  gitStats,
  branches,
  awaitingCount,
  onGit,
  onAwaiting,
  onNewSession,
}: TitlebarProps) {
  const branch = git?.branch ?? null;
  const current = branches.find((entry) => entry.current) ?? null;
  const ahead = current?.ahead ?? 0;
  const changed = git?.files.length ?? 0;
  const showGit = Boolean(session && project?.isGit && branch);

  return (
    <header className="titlebar">
      <div className="brand">
        <span className="brand-mark">
          <BrandMark />
        </span>
        <span className="brand-name">Grove</span>
      </div>

      {showGit ? (
        <button
          className="git-chip"
          type="button"
          title="Open Git changes"
          onClick={onGit}
        >
          <Icon name="branch" size="xs" />
          <span className="gc-branch">{branch}</span>
          <span className="gc-sep" aria-hidden="true" />
          {changed > 0 ? (
            <span className="gc-diff">
              <span className="add">+{gitStats?.added ?? 0}</span>
              <span className="del">−{gitStats?.removed ?? 0}</span>
            </span>
          ) : (
            <span className="gc-diff clean">clean</span>
          )}
          {ahead > 0 ? <span className="gc-ahead">↑{ahead}</span> : null}
        </button>
      ) : null}

      <div className="titlebar-right">
        {awaitingCount > 0 ? (
          <button
            className="await-chip"
            type="button"
            title="Open the session that needs you"
            aria-label={`${awaitingCount} awaiting you — open that session`}
            onClick={onAwaiting}
          >
            <span className="dot" data-state="waiting" aria-hidden="true" />
            <span className="await-text">{awaitingCount} awaiting you</span>
          </button>
        ) : null}
        <button className="btn-primary" type="button" onClick={onNewSession}>
          <Icon name="plus" size="sm" />
          New session
        </button>
      </div>
    </header>
  );
}
