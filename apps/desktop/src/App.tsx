import type { AppInfo } from "@grove/contract";
import { useEffect, useState } from "react";
import { useBridge } from "./ipc/context";
import { errorText } from "./ipc/errors";
import { panels } from "./panels/registry";
import { useUiStore } from "./state/uiStore";

/** The shell: navigation, the active panel, and the core's identity. Panels own everything else. */
export default function App() {
  const bridge = useBridge();
  const activePanel = useUiStore((state) => state.activePanel);
  const setActivePanel = useUiStore((state) => state.setActivePanel);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    bridge
      .invoke("app_info")
      .then((next) => {
        if (!cancelled) setInfo(next);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(errorText(err));
      });
    return () => {
      cancelled = true;
    };
  }, [bridge]);

  const active = panels.find((panel) => panel.id === activePanel) ?? panels[0];
  const ActivePanel = active.Component;

  return (
    <div className="app-shell" data-testid="app-shell">
      <nav className="sidebar" aria-label="Panels">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true">
            ◆
          </span>
          <span>Grove</span>
        </div>
        <ul className="nav-list">
          {panels.map((panel) => {
            const isActive = panel.id === active.id;
            return (
              <li key={panel.id}>
                <button
                  type="button"
                  className={isActive ? "nav-button is-active" : "nav-button"}
                  aria-current={isActive ? "page" : undefined}
                  onClick={() => setActivePanel(panel.id)}
                >
                  <span className="nav-glyph" aria-hidden="true">
                    {panel.glyph}
                  </span>
                  <span>{panel.title}</span>
                </button>
              </li>
            );
          })}
        </ul>
        <div className="sidebar-footer" data-testid="app-info">
          {info ? `${info.name} v${info.version} · ${info.platform}` : error ? "core offline" : "connecting…"}
        </div>
      </nav>
      <main className="panel-area" data-testid={`panel-${active.id}`}>
        <ActivePanel />
      </main>
    </div>
  );
}
