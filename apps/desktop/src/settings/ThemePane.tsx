import type { CSSProperties } from "react";
import { useBridge } from "../ipc/context";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore } from "../state/workspaceStore";
import { findTheme, THEMES, type ThemeMode } from "../workspace/theme";

/** Seven themes, each with its own dark and light column. */
export function ThemePane() {
  const bridge = useBridge();
  const themeId = useWorkspaceStore((state) => state.themeId);
  const mode = useWorkspaceStore((state) => state.themeMode);
  const setTheme = useWorkspaceStore((state) => state.setTheme);
  const saveSettings = useWorkspaceStore((state) => state.saveSettings);
  const notify = useUiStore((state) => state.notify);

  const apply = (id: typeof themeId, nextMode: ThemeMode, name: string) => {
    setTheme(id, nextMode);
    void saveSettings(bridge, { themeId: id, themeMode: nextMode, theme: nextMode })
      .then(() => notify(`${name} · ${nextMode} applied`))
      .catch(() => notify(`${name} · ${nextMode} applied for this run`, "error"));
  };

  return (
    <>
      <div className="sk-head">
        <div className="sk-head-text">
          <h3>Theme</h3>
          <p>
            Seven complete languages. Each has its own dark and light column, its own type and its
            own corner treatment, so switching changes the whole window rather than tinting it.
          </p>
        </div>
        <div className="sk-head-tools">
          <div className="seg" role="group" aria-label="Theme mode">
            <button
              type="button"
              aria-pressed={mode === "dark"}
              onClick={() => apply(themeId, "dark", findTheme(themeId).name)}
            >
              Dark
            </button>
            <button
              type="button"
              aria-pressed={mode === "light"}
              onClick={() => apply(themeId, "light", findTheme(themeId).name)}
            >
              Light
            </button>
          </div>
        </div>
      </div>
      <div className="th-scroll od-scroll">
        <div className="th-grid">
          {THEMES.map((theme) => {
            const preview = theme.preview[mode];
            const style = {
              "--pv-bg": preview.bg,
              "--pv-fg": preview.fg,
              "--pv-fg2": preview.fg2,
              "--pv-accent": preview.accent,
              "--pv-ink": preview.ink,
              "--pv-warn": preview.warn,
              "--pv-line": preview.line,
              "--pv-r": `${theme.radius}px`,
              "--pv-font": `'${preview.font}', sans-serif`,
            } as CSSProperties;
            return (
              <button
                className="th-card"
                type="button"
                key={theme.id}
                aria-pressed={theme.id === themeId}
                onClick={() => apply(theme.id, mode, theme.name)}
              >
                <span className="th-top">
                  <span className="th-name">{theme.name}</span>
                  {theme.id === themeId ? <span className="th-pick">in use</span> : null}
                </span>
                <span className="th-meta">
                  {theme.fonts} · {theme.radius}px
                </span>
                <span className="th-pv" style={style}>
                  <span className="th-pv-bar">
                    <span className="nm">OAuth device flow</span>
                    <span className="st">awaiting</span>
                  </span>
                  <span className="th-pv-row">
                    <span className="th-pv-btn">Approve</span>
                    <span className="th-pv-btn ghost">Reject</span>
                  </span>
                  <span className="th-pv-line">
                    <i />
                  </span>
                </span>
                <span className="th-swatch">
                  {mode === "light"
                    ? [
                        preview.bg,
                        preview.line,
                        preview.accent,
                        preview.warn,
                        preview.fg,
                      ].map((color, index) => <i key={index} style={{ background: color }} />)
                    : theme.swatch.map((color, index) => (
                        <i key={index} style={{ background: color }} />
                      ))}
                </span>
              </button>
            );
          })}
        </div>
        <p className="th-note">
          Applies immediately and is remembered on this machine · saved locally, no account needed.
        </p>
      </div>
    </>
  );
}
