import { useEffect, useRef, useState } from "react";
import { errorText } from "../ipc/errors";
import { useWorkspaceStore } from "../state/workspaceStore";
import type { PendingUpdate } from "../update/updater";

type Status =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current"; checked: string }
  | { kind: "available"; update: PendingUpdate }
  | { kind: "downloading"; update: PendingUpdate; percent: number | null }
  | { kind: "installing"; update: PendingUpdate }
  | { kind: "error"; message: string };

function statusText(status: Status): string {
  switch (status.kind) {
    case "idle":
      return "No check yet. Nothing is downloaded until you choose to install.";
    case "checking":
      return "Asking GitHub for the latest release…";
    case "current":
      return `Up to date, checked at ${status.checked}.`;
    case "available":
      return `Grove ${status.update.version} is available.`;
    case "downloading":
      return status.percent === null
        ? "Downloading the update…"
        : `Downloading the update… ${status.percent}%`;
    case "installing":
      return "Installing and restarting…";
    case "error":
      return status.message;
  }
}

/** Version, update check and install. Updates are signed by the release workflow. */
export function AboutPane() {
  const info = useWorkspaceStore((state) => state.info);
  const [status, setStatus] = useState<Status>({ kind: "idle" });
  const held = useRef<PendingUpdate | null>(null);

  // The plugin keeps the update as a backend resource; release it if the pane closes uninstalled.
  useEffect(
    () => () => {
      void held.current?.dismiss();
    },
    [],
  );

  const version = info?.version ?? "unknown";
  const busy = status.kind === "checking" || status.kind === "downloading";

  const checkNow = async () => {
    setStatus({ kind: "checking" });
    try {
      if (held.current) {
        await held.current.dismiss().catch(() => undefined);
        held.current = null;
      }
      // Lazy so the updater plugin stays out of the entry chunk until someone asks for it.
      const { checkForUpdate } = await import("../update/updater");
      const update = await checkForUpdate();
      held.current = update;
      if (!update) {
        setStatus({ kind: "current", checked: new Date().toLocaleTimeString() });
        return;
      }
      setStatus({ kind: "available", update });
    } catch (error) {
      setStatus({ kind: "error", message: errorText(error) });
    }
  };

  const install = async (update: PendingUpdate) => {
    setStatus({ kind: "downloading", update, percent: null });
    try {
      await update.install((percent) =>
        setStatus({ kind: "downloading", update, percent }),
      );
      held.current = null;
      setStatus({ kind: "installing", update });
      // Windows exits into the installer; this covers the platforms we add later.
      const { restartForUpdate } = await import("../update/updater");
      await restartForUpdate();
    } catch (error) {
      setStatus({ kind: "error", message: errorText(error) });
    }
  };

  const available =
    status.kind === "available" ||
    status.kind === "downloading" ||
    status.kind === "installing"
      ? status.update
      : null;

  return (
    <div className="sm-pane-scroll">
      <div className="sm-pane-head">
        <h3>About</h3>
        <p>
          Grove {version}. Updates come from GitHub Releases and are signed before they are
          installed.
        </p>
      </div>
      <div className="sm-form">
        <div className="field">
          <label htmlFor="about-version">Version</label>
          <input
            id="about-version"
            className="mono-input"
            type="text"
            readOnly
            value={version}
          />
          <p className="field-hint">
            The status bar shows the same version, reported by the Rust core.
          </p>
        </div>
        <div className="field">
          <label htmlFor="about-check">Updates</label>
          <div className="about-actions">
            <button
              id="about-check"
              className="btn-primary"
              type="button"
              disabled={busy || status.kind === "installing"}
              onClick={() => void checkNow()}
            >
              {status.kind === "checking" ? "Checking…" : "Check for updates"}
            </button>
            {available && status.kind === "available" ? (
              <button
                className="btn-primary"
                type="button"
                onClick={() => void install(available)}
              >
                Install {available.version} and restart
              </button>
            ) : null}
          </div>
          <p className="field-hint" role="status">
            {statusText(status)}
          </p>
          {available ? (
            <pre className="about-notes">{available.notes}</pre>
          ) : null}
        </div>
      </div>
    </div>
  );
}
