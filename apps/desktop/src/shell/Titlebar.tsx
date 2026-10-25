import type { GitBranch, GitStatus, Project, SessionDetail } from "@grove/contract";
import { useEffect, useState } from "react";
import { useBridge } from "../ipc/context";
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
}

/** Brand, the active checkout chip, the window controls, and no native frame above them. */
export function Titlebar({
  project,
  session,
  git,
  gitStats,
  branches,
  awaitingCount,
  onGit,
  onAwaiting,
}: TitlebarProps) {
  const bridge = useBridge();
  const [maximized, setMaximized] = useState(false);
  const branch = git?.branch ?? null;
  const current = branches.find((entry) => entry.current) ?? null;
  const ahead = current?.ahead ?? 0;
  const changed = git?.files.length ?? 0;
  const showGit = Boolean(session && project?.isGit && branch);

  useEffect(() => {
    let active = true;
    let timer: number | undefined;
    const sync = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        void bridge
          .invoke("app_window_is_maximized")
          .then((value) => {
            if (active) setMaximized(value);
          })
          .catch(() => {
            // A headless webview has no window to ask about; leave the maximize glyph in place.
          });
      }, 80);
    };
    void bridge
      .invoke("app_window_is_maximized")
      .then((value) => {
        if (active) setMaximized(value);
      })
      .catch(() => {});
    window.addEventListener("resize", sync);
    return () => {
      active = false;
      window.clearTimeout(timer);
      window.removeEventListener("resize", sync);
    };
  }, [bridge]);

  const minimize = () => {
    void bridge.invoke("app_window_minimize").catch(() => {});
  };

  const toggleMaximize = async () => {
    try {
      setMaximized(await bridge.invoke("app_window_toggle_maximize"));
    } catch {
      // Keep the current glyph if the platform refuses the request.
    }
  };

  const close = () => {
    void bridge.invoke("app_window_close").catch(() => {});
  };

  return (
    <header className="titlebar">
      <div
        className="brand"
        data-tauri-drag-region
        onDoubleClick={() => void toggleMaximize()}
      >
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

      <div
        className="titlebar-drag"
        data-tauri-drag-region
        onDoubleClick={() => void toggleMaximize()}
      />

      <div className="titlebar-right">
        {awaitingCount > 0 ? (
          <button
            className="await-chip"
            type="button"
            title="Open the session that needs you"
            aria-label={`${awaitingCount} awaiting you - open that session`}
            onClick={onAwaiting}
          >
            <span className="dot" data-state="waiting" aria-hidden="true" />
            <span className="await-text">{awaitingCount} awaiting you</span>
          </button>
        ) : null}
        <div className="window-controls" role="group" aria-label="Window controls">
          <button
            className="window-btn"
            type="button"
            aria-label="Minimize window"
            title="Minimize"
            onClick={minimize}
          >
            <Icon name="minimize" size="sm" />
          </button>
          <button
            className="window-btn"
            type="button"
            aria-label={maximized ? "Restore window" : "Maximize window"}
            title={maximized ? "Restore" : "Maximize"}
            onClick={() => void toggleMaximize()}
          >
            <Icon name={maximized ? "restore" : "maximize"} size="sm" />
          </button>
          <button
            className="window-btn close"
            type="button"
            aria-label="Close window"
            title="Close"
            onClick={close}
          >
            <Icon name="x" size="sm" />
          </button>
        </div>
      </div>
    </header>
  );
}
