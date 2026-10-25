import type {
  DoctorReport,
  HarnessInfo,
  SecretStatusView,
  Settings,
  Theme,
} from "@grove/contract";
import { useCallback, useEffect, useState } from "react";
import { useBridge } from "../../ipc/context";
import { errorText } from "../../ipc/errors";
import { PanelFrame } from "../PanelFrame";
import "./settings.css";

/** Must match `grove_core::secrets::GITHUB_TOKEN`; the only secret the panel offers today. */
const GITHUB_TOKEN_KEY = "github.token";
const THEMES: Theme[] = ["system", "light", "dark"];
const THEME_LABELS: Record<Theme, string> = {
  system: "System",
  light: "Light",
  dark: "Dark",
};

interface SaveState {
  kind: "success" | "error";
  message: string;
}


function secretStatusLabel(status: SecretStatusView["status"] | undefined): string {
  switch (status) {
    case "present":
      return "Saved in keychain";
    case "unavailable":
      return "Keychain unavailable";
    default:
      return "Not set";
  }
}

/**
 * Settings: harness binary overrides, catalog sources and theme, the keychain-backed GitHub token
 * (status only — the value is never rendered once saved), and the doctor table.
 *
 * Everything goes through `useBridge()`; the panel never imports Tauri and stays testable under
 * `FakeBridge`.
 */
export function SettingsPanel() {
  const bridge = useBridge();

  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [harnessIds, setHarnessIds] = useState<string[]>([]);
  const [paths, setPaths] = useState<Record<string, string>>({});
  const [sources, setSources] = useState("");
  const [theme, setTheme] = useState<Theme>("system");
  const [saveState, setSaveState] = useState<SaveState | null>(null);
  const [saving, setSaving] = useState(false);

  const [secret, setSecret] = useState<SecretStatusView | null>(null);
  const [token, setToken] = useState("");
  const [secretBusy, setSecretBusy] = useState(false);
  const [secretError, setSecretError] = useState<string | null>(null);

  const [report, setReport] = useState<DoctorReport | null>(null);
  const [doctorBusy, setDoctorBusy] = useState(false);
  const [doctorError, setDoctorError] = useState<string | null>(null);

  const applySettings = useCallback((next: Settings) => {
    setPaths({ ...next.harnessPaths });
    setSources(next.catalogSources.join("\n"));
    setTheme(next.theme);
  }, []);

  const loadSettings = useCallback(async () => {
    setLoadError(null);
    try {
      const [next, harnesses] = await Promise.all([
        bridge.invoke("settings_get"),
        bridge.invoke("harness_list"),
      ]);
      applySettings(next);
      setHarnessIds(harnesses.map((harness: HarnessInfo) => harness.id));
      setLoaded(true);
    } catch (error) {
      setLoadError(errorText(error));
    }
  }, [applySettings, bridge]);

  const loadSecret = useCallback(async () => {
    setSecretError(null);
    try {
      setSecret(await bridge.invoke("secret_status", { key: GITHUB_TOKEN_KEY }));
    } catch (error) {
      setSecretError(errorText(error));
    }
  }, [bridge]);

  const loadDoctor = useCallback(async () => {
    setDoctorBusy(true);
    setDoctorError(null);
    try {
      setReport(await bridge.invoke("doctor_report"));
    } catch (error) {
      setDoctorError(errorText(error));
    } finally {
      setDoctorBusy(false);
    }
  }, [bridge]);

  useEffect(() => {
    void loadSettings();
    void loadSecret();
    void loadDoctor();
  }, [loadDoctor, loadSecret, loadSettings]);

  async function saveSettings() {
    setSaving(true);
    setSaveState(null);
    const harnessPaths = Object.fromEntries(
      Object.entries(paths)
        .map(([id, path]) => [id, path.trim()])
        .filter(([, path]) => path !== ""),
    );
    const catalogSources = sources
      .split("\n")
      .map((source) => source.trim())
      .filter((source) => source !== "");
    try {
      const next = await bridge.invoke("settings_set", {
        patch: { harnessPaths, catalogSources, theme },
      });
      applySettings(next);
      setSaveState({ kind: "success", message: "Settings saved." });
      // Paths may have changed; the doctor table is stale now.
      void loadDoctor();
    } catch (error) {
      setSaveState({ kind: "error", message: errorText(error) });
    } finally {
      setSaving(false);
    }
  }

  async function saveToken() {
    const value = token.trim();
    if (!value) return;
    setSecretBusy(true);
    setSecretError(null);
    try {
      setSecret(await bridge.invoke("secret_set", { key: GITHUB_TOKEN_KEY, value }));
      // Never keep the value around after it is in the keychain.
      setToken("");
    } catch (error) {
      setSecretError(errorText(error));
    } finally {
      setSecretBusy(false);
    }
  }

  async function deleteToken() {
    setSecretBusy(true);
    setSecretError(null);
    try {
      setSecret(await bridge.invoke("secret_delete", { key: GITHUB_TOKEN_KEY }));
    } catch (error) {
      setSecretError(errorText(error));
    } finally {
      setSecretBusy(false);
    }
  }

  const pathRows = Array.from(new Set([...harnessIds, ...Object.keys(paths)])).sort();
  const doctorRows = report ? [...report.harnesses, ...report.tools] : [];
  const secretStatus = secret?.status;

  return (
    <PanelFrame
      title="Settings"
      subtitle="Harness binaries, catalog sources, tokens kept in the OS keychain, and what is actually installed."
      actions={
        <button
          type="button"
          className="input"
          onClick={() => void loadDoctor()}
          disabled={doctorBusy}
        >
          Refresh doctor
        </button>
      }
    >
      <div className="settings-panel">
        {loadError ? (
          <p className="settings-banner is-error" data-testid="settings-error">
            Settings unavailable: {loadError}
          </p>
        ) : null}

        <section className="settings-section" aria-label="Harness paths">
          <h2>Harness binaries</h2>
          <p className="settings-muted">
            Leave a path empty to resolve the harness on <code>PATH</code>. An override is used
            exactly as written; a wrong path is reported by the doctor below.
          </p>
          {!loaded && !loadError ? (
            <p className="settings-muted">Loading settings…</p>
          ) : (
            <div className="settings-rows">
              {pathRows.length === 0 ? (
                <p className="settings-muted">No harnesses are registered.</p>
              ) : (
                pathRows.map((id) => (
                  <label className="settings-row" key={id}>
                    <span className="settings-row-label">{id}</span>
                    <input
                      className="input settings-row-input"
                      type="text"
                      aria-label={`Path for ${id}`}
                      placeholder="C:/path/to/binary (default: PATH)"
                      value={paths[id] ?? ""}
                      onChange={(event) =>
                        setPaths((current) => ({ ...current, [id]: event.target.value }))
                      }
                      spellCheck={false}
                    />
                  </label>
                ))
              )}
            </div>
          )}
        </section>

        <section className="settings-section" aria-label="Catalog and theme">
          <h2>Catalog and appearance</h2>
          <label className="settings-field">
            <span>Catalog sources</span>
            <textarea
              className="input"
              aria-label="Catalog sources"
              rows={3}
              placeholder="owner/name, one per line"
              value={sources}
              onChange={(event) => setSources(event.target.value)}
              spellCheck={false}
            />
          </label>
          <p className="settings-muted">
            <code>owner/name</code> GitHub repositories, one per line.
          </p>
          <label className="settings-field settings-theme">
            <span>Theme</span>
            <select
              className="input"
              aria-label="Theme"
              value={theme}
              onChange={(event) => setTheme(event.target.value as Theme)}
            >
              {THEMES.map((candidate) => (
                <option key={candidate} value={candidate}>
                  {THEME_LABELS[candidate]}
                </option>
              ))}
            </select>
          </label>
          <div className="settings-actions">
            <button
              type="button"
              className="input primary"
              onClick={() => void saveSettings()}
              disabled={saving || !loaded}
            >
              Save settings
            </button>
            {saveState ? (
              <p
                className={`settings-banner ${
                  saveState.kind === "success" ? "is-success" : "is-error"
                }`}
                data-testid="settings-save"
              >
                {saveState.message}
              </p>
            ) : null}
          </div>
        </section>

        <section className="settings-section" aria-label="Secrets">
          <h2>GitHub token</h2>
          <p className="settings-muted">
            Stored in the OS keychain. Once saved it is never shown again — only its status is.
          </p>
          <div className="settings-actions">
            <span
              className={`settings-chip is-${secretStatus ?? "absent"}`}
              data-testid="secret-status"
              data-status={secretStatus ?? "absent"}
            >
              {secretStatusLabel(secretStatus)}
            </span>
            <input
              className="input settings-token-input"
              type="password"
              aria-label="GitHub token"
              placeholder="Paste a token to store it"
              value={token}
              onChange={(event) => setToken(event.target.value)}
              autoComplete="off"
              spellCheck={false}
            />
            <button
              type="button"
              className="input primary"
              onClick={() => void saveToken()}
              disabled={secretBusy || token.trim() === ""}
            >
              Save token
            </button>
            <button
              type="button"
              className="input"
              onClick={() => void deleteToken()}
              disabled={secretBusy || secretStatus !== "present"}
            >
              Delete token
            </button>
          </div>
          {secretError ? (
            <p className="settings-banner is-error" data-testid="secret-error">
              Secret store unavailable: {secretError}
            </p>
          ) : null}
        </section>

        <section className="settings-section" aria-label="Harness doctor">
          <h2>Doctor</h2>
          <p className="settings-muted">
            {report
              ? `Checked ${report.checkedAt}`
              : doctorBusy
                ? "Checking installed binaries…"
                : "No report yet."}
          </p>
          {doctorError ? (
            <p className="settings-banner is-error" data-testid="doctor-error">
              Doctor unavailable: {doctorError}
            </p>
          ) : null}
          <table className="settings-table" data-testid="doctor-report">
            <thead>
              <tr>
                <th scope="col">Component</th>
                <th scope="col">Status</th>
                <th scope="col">Version</th>
                <th scope="col">Path</th>
                <th scope="col">Detail</th>
              </tr>
            </thead>
            <tbody>
              {doctorRows.map((row) => (
                <tr key={`${row.id}-${row.label}`} data-testid={`doctor-${row.id}`}>
                  <td>
                    <span className="settings-row-label">{row.label}</span>
                    <span className="settings-muted"> {row.id}</span>
                  </td>
                  <td>
                    <span className={`settings-chip is-${row.installed ? "present" : "absent"}`}>
                      {row.installed ? "installed" : "missing"}
                    </span>
                  </td>
                  <td>{row.version ?? "—"}</td>
                  <td className="settings-path">{row.path ?? "—"}</td>
                  <td className="settings-detail">{row.detail ?? ""}</td>
                </tr>
              ))}
              {doctorRows.length === 0 ? (
                <tr>
                  <td colSpan={5} className="settings-muted">
                    {doctorBusy ? "Running doctor…" : "Nothing reported yet."}
                  </td>
                </tr>
              ) : null}
            </tbody>
          </table>
        </section>
      </div>
    </PanelFrame>
  );
}
