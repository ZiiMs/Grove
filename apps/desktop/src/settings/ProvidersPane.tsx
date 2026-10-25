import type { ProviderStatus } from "@grove/contract";
import { Fragment, useCallback, useEffect, useState } from "react";
import { Dialog } from "../dialogs/Dialog";
import { useBridge } from "../ipc/context";
import { errorText, toCommandFailure } from "../ipc/errors";
import { useSessionStore } from "../state/sessionStore";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore } from "../state/workspaceStore";
import { Icon } from "../workspace/Icon";
import {
  credentialOrigin,
  filterProviders,
  providerCounts,
  providerState,
  validateProviderKey,
} from "./providerView";

type DialogView =
  | { step: "browse" }
  | { step: "key"; providerId: string; from: "browse" | "manage" }
  | { step: "manage"; providerId: string };

/** What the last test-key call learned about one provider. */
interface Probe {
  error: string | null;
  /** The vendor refused the credential, as opposed to a network or protocol failure. */
  rejected: boolean;
  /** How many models the credential can reach, after a successful test. */
  models: number | null;
}

const EMPTY_PROBE: Probe = { error: null, rejected: false, models: null };

function ProviderMark({ row, size }: { row: ProviderStatus; size?: "lg" }) {
  return (
    <span
      className={`mark provider-mark ${size ?? ""}`}
      style={{ ["--tint" as string]: row.tint }}
      aria-hidden="true"
    >
      {row.short}
    </span>
  );
}

function ProviderDialogHead({
  row,
  large,
  title,
  subtitle,
  closeDisabled,
  onClose,
}: {
  row?: ProviderStatus;
  large?: boolean;
  title: string;
  subtitle: string;
  closeDisabled: boolean;
  onClose: () => void;
}) {
  return (
    <div className="dlg-head pd-head">
      {row ? <ProviderMark row={row} size={large ? "lg" : undefined} /> : null}
      <div className="pd-head-copy">
        <h2 id="provider-dialog-title">{title}</h2>
        <p className="dlg-sub">{subtitle}</p>
      </div>
      <button
        className="icon-btn sm"
        type="button"
        aria-label="Close dialog"
        disabled={closeDisabled}
        onClick={onClose}
      >
        <Icon name="x" size="sm" />
      </button>
    </div>
  );
}

/**
 * Providers: the credentials this machine can run sessions on.
 *
 * There is nothing to install — Grove runs the agent and only needs a key. Cards are the providers
 * that are configured; the rest of the catalogue lives behind the connect card. A key is written
 * straight to the OS keychain through `provider_key_set`, and a provider reports only whether one
 * exists, never its value.
 */
export function ProvidersPane() {
  const bridge = useBridge();
  const prefs = useWorkspaceStore((state) => state.prefs);
  const saveSettings = useWorkspaceStore((state) => state.saveSettings);
  const reloadSessions = useSessionStore((state) => state.load);
  const notify = useUiStore((state) => state.notify);

  const [rows, setRows] = useState<ProviderStatus[] | null>(null);
  const [view, setView] = useState<DialogView | null>(null);
  const [query, setQuery] = useState("");
  const [draftKey, setDraftKey] = useState("");
  const [keyError, setKeyError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [probes, setProbes] = useState<Record<string, Probe>>({});

  const saving = busy !== null && view?.step === "key";

  const loadStatuses = useCallback(async () => {
    try {
      const statuses = await bridge.invoke("provider_statuses");
      setRows(statuses);
    } catch {
      setRows([]);
    }
  }, [bridge]);

  useEffect(() => {
    void loadStatuses();
  }, [loadStatuses]);

  const probeFor = (providerId: string) => probes[providerId] ?? EMPTY_PROBE;

  const setProbe = (providerId: string, probe: Probe) => {
    setProbes((current) => ({ ...current, [providerId]: probe }));
  };

  const applyRow = (updated: ProviderStatus) => {
    setRows((current) => current?.map((item) => (item.id === updated.id ? updated : item)) ?? null);
    setProbes((current) => {
      const next = { ...current };
      delete next[updated.id];
      return next;
    });
  };

  const closeDialog = () => {
    if (saving) return;
    setView(null);
    setQuery("");
    setDraftKey("");
    setKeyError(null);
  };

  const openBrowse = () => {
    setQuery("");
    setKeyError(null);
    setView({ step: "browse" });
  };

  const openManage = (providerId: string) => {
    setKeyError(null);
    setView({ step: "manage", providerId });
  };

  const openKey = (providerId: string, from: "browse" | "manage") => {
    setDraftKey("");
    setKeyError(null);
    setView({ step: "key", providerId, from });
  };

  const rowFor = (providerId: string) => rows?.find((row) => row.id === providerId) ?? null;

  const saveKey = async (row: ProviderStatus) => {
    const check = validateProviderKey(draftKey);
    if (!check.ok) {
      setKeyError(check.error);
      return;
    }
    setBusy(row.id);
    setKeyError(null);
    try {
      const updated = await bridge.invoke("provider_key_set", { provider: row.id, key: check.key });
      applyRow(updated);
      await reloadSessions(bridge);
      notify(`${updated.name} key saved to the keychain`);
      setDraftKey("");
      setView(null);
    } catch (error) {
      setKeyError(errorText(error));
    } finally {
      setBusy(null);
    }
  };

  /** Ask the vendor for the models this key can reach: the cheapest proof that it works. */
  const testKey = async (row: ProviderStatus) => {
    setBusy(row.id);
    try {
      const models = await bridge.invoke("provider_models", { provider: row.id });
      setProbe(row.id, { error: null, rejected: false, models: models.length });
      notify(`${row.name}: ${models.length} models available`);
    } catch (error) {
      const failure = toCommandFailure(error);
      const message = errorText(error);
      // Unavailable also covers a locked keychain; only a vendor 401/403 is a key rejection.
      const rejected =
        failure.code === "unavailable" && /^HTTP (401|403):/.test(failure.message);
      setProbe(row.id, { error: message, rejected, models: null });
      notify(message, "error");
    } finally {
      setBusy(null);
    }
  };

  const makeDefault = async (row: ProviderStatus) => {
    try {
      await saveSettings(bridge, { defaultProvider: row.id });
      notify(`${row.name} is the default provider`);
    } catch {
      notify("Could not save the default provider", "error");
    }
  };

  const disconnect = async (row: ProviderStatus) => {
    setBusy(row.id);
    try {
      const updated = await bridge.invoke("provider_key_delete", { provider: row.id });
      const fallback =
        !updated.configured && prefs.defaultProvider === updated.id
          ? (rows ?? []).find((item) => item.id !== updated.id && item.configured) ?? null
          : null;
      applyRow(updated);
      await reloadSessions(bridge);
      let message = updated.configured
        ? `${row.name} now uses ${updated.envVar}`
        : `${row.name} key removed`;
      if (fallback) {
        try {
          await saveSettings(bridge, { defaultProvider: fallback.id });
          message += ` · ${fallback.name} is now the default`;
        } catch {
          message += " · the default provider could not be saved";
        }
      }
      notify(message);
      setView(null);
    } catch (error) {
      notify(errorText(error), "error");
    } finally {
      setBusy(null);
    }
  };

  const counts = providerCounts(rows ?? []);
  const connected = (rows ?? []).filter((row) => row.configured);
  const matches = filterProviders(rows ?? [], query);
  const catalogueGroups = [
    { title: "Connected", rows: matches.filter((row) => row.configured) },
    { title: "Available to connect", rows: matches.filter((row) => !row.configured) },
  ].filter((group) => group.rows.length > 0);

  function card(row: ProviderStatus) {
    const probe = probeFor(row.id);
    const state = providerState(row, probe.rejected);
    return (
      <button
        className="provider-card"
        type="button"
        key={row.id}
        data-state={state.tone}
        onClick={() => openManage(row.id)}
      >
        <span className="pc-top">
          <ProviderMark row={row} />
          <span className="pc-name">{row.name}</span>
          {prefs.defaultProvider === row.id ? <span className="pc-default">default</span> : null}
        </span>
        <span className="pc-foot">
          <span className="pc-state" data-state={state.tone}>
            <span className="d" aria-hidden="true" />
            {state.label}
          </span>
          <span className="pc-models">
            <b>{row.models.length}</b> models
          </span>
        </span>
      </button>
    );
  }

  function browseStep() {
    return (
      <div className="pd-shell">
        <ProviderDialogHead
          title="Add a provider"
          subtitle="The providers this build can run. Credentials stay on this machine."
          closeDisabled={saving}
          onClose={closeDialog}
        />

        <div className="pd-search">
          <label className="sr-only" htmlFor="provider-search">
            Search providers
          </label>
          <Icon name="search" size="sm" />
          <input
            id="provider-search"
            type="search"
            autoFocus
            data-autofocus
            autoComplete="off"
            placeholder="Search providers, models, env vars"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>
        <p className="pd-count">
          {matches.length} of {counts.total} providers
          {query.trim() ? ` matching “${query.trim()}”` : ""}
        </p>

        <div className="pd-list">
          {matches.length === 0 ? (
            <div className="pd-empty">
              <strong>No providers match</strong>
              Try a provider name, a model, or an environment variable.
            </div>
          ) : (
            catalogueGroups.map((group) => (
              <Fragment key={group.title}>
                <p className="pd-group">{group.title}</p>
                {group.rows.map((row) => (
                  <button
                    className="pd-row"
                    type="button"
                    key={row.id}
                    disabled={row.configured}
                    onClick={() => openKey(row.id, "browse")}
                  >
                    <ProviderMark row={row} />
                    <span className="pd-row-body">
                      <b>{row.name}</b>
                      <span>
                        {row.configured ? row.defaultModel : (row.detail ?? row.defaultModel)}
                      </span>
                    </span>
                    {row.configured ? (
                      <span className="pd-tag local">connected</span>
                    ) : (
                      <span className="pd-tag">{row.models.length} models</span>
                    )}
                  </button>
                ))}
              </Fragment>
            ))
          )}
        </div>
      </div>
    );
  }

  function keyStep(row: ProviderStatus, from: "browse" | "manage") {
    return (
      <div className="pd-shell">
        <ProviderDialogHead
          row={row}
          large
          title={from === "manage" ? `Replace ${row.name} key` : `Connect ${row.name}`}
          subtitle={`${row.models.length} models · ${row.defaultModel}`}
          closeDisabled={saving}
          onClose={closeDialog}
        />

        <form
          className="pd-body"
          onSubmit={(event) => {
            event.preventDefault();
            void saveKey(row);
          }}
        >
          <div className="field">
            <label htmlFor="provider-key">{row.name} API key</label>
            <input
              id="provider-key"
              className="mono-input"
              type="password"
              autoFocus
              data-autofocus
              autoComplete="off"
              spellCheck={false}
              placeholder="Paste the key"
              disabled={saving}
              value={draftKey}
              onChange={(event) => {
                setDraftKey(event.target.value);
                setKeyError(null);
              }}
            />
            <p className="field-hint">
              Create a key at{" "}
              <a href={row.keyUrl} target="_blank" rel="noreferrer">
                {row.keyUrl}
              </a>
              . It goes to the OS keychain, never settings or logs. You can also export{" "}
              <code>{row.envVar}</code>.
            </p>
            {keyError ? (
              <p className="field-err" role="alert">
                {keyError}
              </p>
            ) : null}
          </div>

          <div className="dlg-foot pd-key-foot">
            <button
              className="btn-ghost"
              type="button"
              disabled={saving}
              onClick={() => {
                setDraftKey("");
                setKeyError(null);
                if (from === "manage") {
                  setView({ step: "manage", providerId: row.id });
                } else {
                  setView({ step: "browse" });
                }
              }}
            >
              Back
            </button>
            <button className="btn-primary" type="submit" disabled={saving}>
              {saving ? "Connecting…" : "Connect"}
            </button>
          </div>
        </form>
      </div>
    );
  }

  function manageStep(row: ProviderStatus) {
    const probe = probeFor(row.id);
    const state = providerState(row, probe.rejected);
    return (
      <div className="pd-shell">
        <ProviderDialogHead
          row={row}
          large
          title={`Manage ${row.name}`}
          subtitle={`${row.models.length} models · API key${
            prefs.defaultProvider === row.id ? " · default" : ""
          }`}
          closeDisabled={saving}
          onClose={closeDialog}
        />

        <div className="pd-body">
          <div className="pd-status" data-state={state.tone}>
            <span className="pd-status-body">
              <b>API key</b>
              <span>{credentialOrigin(row)}</span>
            </span>
            <span className="pd-chip" data-state={state.tone}>
              <span className="d" aria-hidden="true" />
              {state.label}
            </span>
          </div>

          <div className="pd-kv">
            <span>Models</span>
            <b>{row.models.length}</b>
          </div>
          <div className="pd-kv">
            <span>Default model</span>
            <b>{row.defaultModel}</b>
          </div>
          <div className="pd-kv">
            <span>Key</span>
            <b>
              <a href={row.keyUrl} target="_blank" rel="noreferrer">
                {row.keyUrl}
              </a>
            </b>
          </div>
          <div className="pd-kv">
            <span>Fallback</span>
            <b>{row.envVar}</b>
          </div>

          {probe.error ? (
            <p className="pd-note" data-tone="bad">
              {probe.error}
            </p>
          ) : probe.models !== null ? (
            <p className="pd-note" data-tone="ok">
              {probe.models} models available through this key
              {probe.models !== row.models.length
                ? ` · ${row.models.length} in this build`
                : ""}
            </p>
          ) : (
            <p className="pd-note">Test the key to ask the vendor which models it can reach.</p>
          )}

          <div className="pd-actions">
            <button
              className="btn-ghost sm"
              type="button"
              disabled={busy === row.id}
              onClick={() => void testKey(row)}
            >
              {busy === row.id ? "Testing…" : "Test key"}
            </button>
            {prefs.defaultProvider !== row.id ? (
              <button className="btn-ghost sm" type="button" onClick={() => void makeDefault(row)}>
                Make default
              </button>
            ) : null}
            <button className="btn-ghost sm" type="button" onClick={() => openKey(row.id, "manage")}>
              Replace key
            </button>
          </div>
        </div>

        <div className="dlg-foot pd-manage-foot">
          <button
            className="btn-ghost danger"
            type="button"
            disabled={busy === row.id}
            onClick={() => void disconnect(row)}
          >
            Disconnect
          </button>
          <button className="btn-primary" type="button" onClick={closeDialog}>
            Done
          </button>
        </div>
      </div>
    );
  }

  function dialogContent() {
    if (!view) return null;
    if (view.step === "browse") return browseStep();
    const row = rowFor(view.providerId);
    if (!row) return null;
    return view.step === "key" ? keyStep(row, view.from) : manageStep(row);
  }

  return (
    <div className="sm-pane-scroll">
      <div className="sm-pane-head provider-head">
        <div>
          <h3>Agent providers</h3>
          <p>
            The model providers this machine can run sessions on. Pick a card to manage its key, or
            connect another with an API key. Keys stay in the OS keychain.
          </p>
        </div>
        {rows !== null ? (
          <span className="count-pill">
            {counts.connected} connected
            {counts.connected > 0 ? ` · ${counts.models} models` : ""}
          </span>
        ) : null}
      </div>

      {rows === null ? (
        <p className="git-empty">Reading provider credentials…</p>
      ) : (
        <>
          <div className="provider-grid">
            {connected.map(card)}
            <button className="provider-add" type="button" onClick={openBrowse}>
              <span className="provider-add-plus" aria-hidden="true">
                +
              </span>
              <b>Connect provider</b>
              <span>{counts.available} available</span>
            </button>
          </div>

          <p className="provider-note">
            {counts.connected} of {counts.total} connected · {counts.available} available to add
          </p>
        </>
      )}

      {view ? (
        <Dialog
          open
          className="provider-dialog"
          labelledBy="provider-dialog-title"
          onClose={closeDialog}
        >
          {dialogContent()}
        </Dialog>
      ) : null}
    </div>
  );
}
