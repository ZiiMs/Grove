import { useEffect } from "react";
import { useBridge } from "../ipc/context";
import { useSkillsStore } from "../state/skillsStore";
import { useSessionStore } from "../state/sessionStore";
import { useUiStore, type SettingsTab } from "../state/uiStore";
import { Icon } from "../workspace/Icon";
import { Dialog } from "../dialogs/Dialog";
import { GeneralPane } from "./GeneralPane";
import { ProvidersPane } from "./ProvidersPane";
import { McpPane } from "./McpPane";
import { SkillsPane } from "./SkillsPane";
import { ThemePane } from "./ThemePane";

const TABS: { id: SettingsTab; label: string }[] = [
  { id: "general", label: "General" },
  { id: "theme", label: "Theme" },
  { id: "skills", label: "Skills" },
  { id: "providers", label: "Providers" },
  { id: "mcp", label: "MCP" },
];

/** The near-fullscreen settings sheet: defaults, themes, catalog, providers and MCP. */
export function SettingsModal() {
  const bridge = useBridge();
  const tab = useUiStore((state) => state.settingsTab);
  const openSettings = useUiStore((state) => state.openSettings);
  const close = useUiStore((state) => state.closeSettings);
  const installs = useSkillsStore((state) => state.installs);
  const inventory = useSkillsStore((state) => state.inventory);
  const loadInstalls = useSkillsStore((state) => state.loadInstalls);
  const session = useSessionStore((state) => state.detail);
  const projectRoot = session?.cwd;
  const installedCount = new Set([
    ...installs.map((record) => record.skill.trim().toLowerCase()),
    ...inventory.map((item) => item.name.trim().toLowerCase()),
  ]).size;

  useEffect(() => {
    if (tab !== null) void loadInstalls(bridge, projectRoot);
  }, [bridge, loadInstalls, projectRoot, tab]);

  return (
    <Dialog
      open={tab !== null}
      className="settings-modal"
      labelledBy="set-title"
      onClose={close}
    >
      <div className="sm-shell">
        <header className="sm-head">
          <h2 id="set-title">Settings</h2>
          <button className="icon-btn sm" type="button" aria-label="Close settings" onClick={close}>
            <Icon name="x" size="sm" />
          </button>
        </header>
        <div className="sm-body">
          <nav className="sm-nav" role="tablist" aria-label="Settings categories">
            {TABS.map((entry) => (
              <button
                key={entry.id}
                type="button"
                role="tab"
                aria-selected={tab === entry.id}
                aria-controls={`sm-${entry.id}`}
                onClick={() => openSettings(entry.id)}
              >
                {entry.label}
                {entry.id === "skills" && installedCount > 0 ? (
                  <span className="badge">{installedCount}</span>
                ) : null}
              </button>
            ))}
          </nav>
          <div className="sm-panes">
            {tab === "general" ? (
              <section className="sm-pane" id="sm-general" role="tabpanel" aria-label="General">
                <GeneralPane onSaved={close} />
              </section>
            ) : null}
            {tab === "theme" ? (
              <section className="sm-pane" id="sm-theme" role="tabpanel" aria-label="Theme">
                <ThemePane />
              </section>
            ) : null}
            {tab === "skills" ? (
              <section className="sm-pane" id="sm-skills" role="tabpanel" aria-label="Skills">
                <SkillsPane projectRoot={projectRoot} />
              </section>
            ) : null}
            {tab === "providers" ? (
              <section className="sm-pane" id="sm-providers" role="tabpanel" aria-label="Providers">
                <ProvidersPane />
              </section>
            ) : null}
            {tab === "mcp" ? (
              <section className="sm-pane" id="sm-mcp" role="tabpanel" aria-label="MCP servers">
                <McpPane />
              </section>
            ) : null}
          </div>
        </div>
      </div>
    </Dialog>
  );
}
