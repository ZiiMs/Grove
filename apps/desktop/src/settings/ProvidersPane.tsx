import type { ProviderStatus } from "@grove/contract";
import { useCallback, useEffect, useState } from "react";
import { useBridge } from "../ipc/context";
import { useSessionStore } from "../state/sessionStore";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore } from "../state/workspaceStore";
import { providerMeta } from "../workspace/models";
import { Icon } from "../workspace/Icon";

function Mark({ provider }: { provider: string }) {
  const meta = providerMeta(provider);
  return (
    <span className="mark lg" style={{ ["--tint" as string]: meta.tint }} aria-hidden="true">
      {meta.short}
    </span>
  );
}

/**
 * Providers: one row per vendor, with the state of its credential.
 *
 * There is nothing to install — Grove runs the agent and only needs a key. A key is written straight
 * to the OS keychain through `provider_key_set`, and a provider reports only whether one exists,
 * never its value.
 */
export function ProvidersPane() {
  const bridge = useBridge();
  const prefs = useWorkspaceStore((state) => state.prefs);
  const saveSettings = useWorkspaceStore((state) => state.saveSettings);
  const reloadSessions = useSessionStore((state) => state.load);
  const notify = useUiStore((state) => state.notify);

  const [rows, setRows] = useState<ProviderStatus[] | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState<string | null>(null);

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

  const configured = (rows ?? []).filter((row) => row.configured).map((row) => row.id);

  const save = async (provider: string) => {
    const key = draft.trim();
    if (!key) return;
    setBusy(provider);
    try {
      const row = await bridge.invoke("provider_key_set", { provider, key });
      setRows((current) => current?.map((item) => (item.id === provider ? row : item)) ?? null);
      setEditing(null);
      setDraft("");
      await reloadSessions(bridge);
      notify(`${row.name} key saved to the keychain`);
    } catch (error) {
      notify(error instanceof Error ? error.message : `Could not save the ${provider} key`, "error");
    } finally {
      setBusy(null);
    }
  };

  const remove = async (provider: string) => {
    setBusy(provider);
    try {
      const row = await bridge.invoke("provider_key_delete", { provider });
      setRows((current) => current?.map((item) => (item.id === provider ? row : item)) ?? null);
      await reloadSessions(bridge);
      notify(`${row.name} key removed`);
    } catch {
      notify(`Could not remove the ${provider} key`, "error");
    } finally {
      setBusy(null);
    }
  };

  /** Ask the vendor for the models this key can reach: the cheapest proof that it works. */
  const test = async (provider: string) => {
    setBusy(provider);
    try {
      const models = await bridge.invoke("provider_models", { provider });
      notify(`${providerMeta(provider).name}: ${models.length} models available`);
    } catch (error) {
      notify(error instanceof Error ? error.message : "The key was rejected", "error");
    } finally {
      setBusy(null);
    }
  };

  const makeDefault = async (provider: string) => {
    try {
      await saveSettings(bridge, { defaultProvider: provider });
      notify(`${providerMeta(provider).name} is the default provider`);
    } catch {
      notify("Could not save the default provider", "error");
    }
  };

  return (
    <div className="sm-pane-scroll">
      <div className="sm-pane-head">
        <h3>Providers</h3>
        <p>
          Grove runs the agent itself, so a session needs a model provider and a key — not a CLI to
          install. Keys go to the OS keychain and are never written to settings or logs.
        </p>
      </div>

      <div className="provider-list">
        {(rows ?? []).map((row) => {
          const isDefault = prefs.defaultProvider === row.id;
          const isEditing = editing === row.id;
          return (
            <div className="provider-row" key={row.id}>
              <Mark provider={row.id} />
              <div className="hr-body">
                <span className="hr-name">
                  {row.name}
                  {isDefault ? <span className="hr-default">default</span> : null}
                </span>
                <span className="hr-meta">
                  {row.configured
                    ? `${row.source === "environment" ? `from ${row.envVar}` : "key in the keychain"} · ${row.defaultModel}`
                    : row.detail}
                </span>
                {isEditing ? (
                  <form
                    className="hr-key-form"
                    onSubmit={(event) => {
                      event.preventDefault();
                      void save(row.id);
                    }}
                  >
                    <input
                      className="input mono"
                      type="password"
                      autoFocus
                      autoComplete="off"
                      spellCheck={false}
                      placeholder={`${row.name} API key`}
                      aria-label={`${row.name} API key`}
                      value={draft}
                      onChange={(event) => setDraft(event.target.value)}
                    />
                    <button className="btn-primary" type="submit" disabled={busy === row.id || !draft.trim()}>
                      Save
                    </button>
                    <button
                      className="link-button"
                      type="button"
                      onClick={() => {
                        setEditing(null);
                        setDraft("");
                      }}
                    >
                      Cancel
                    </button>
                    <a className="link-button" href={row.keyUrl} target="_blank" rel="noreferrer">
                      Get a key
                    </a>
                  </form>
                ) : null}
              </div>
              <span className="hr-state">
                {row.configured ? "ready" : "needs a key"}
              </span>
              <div className="hr-actions">
                <button
                  className="link-button"
                  type="button"
                  onClick={() => {
                    setEditing(row.id);
                    setDraft("");
                  }}
                  disabled={busy === row.id}
                >
                  {row.configured ? "Replace key" : "Add key"}
                </button>
                {row.configured ? (
                  <>
                    <button
                      className="link-button"
                      type="button"
                      onClick={() => void test(row.id)}
                      disabled={busy === row.id}
                    >
                      Test
                    </button>
                    <button
                      className="link-button"
                      type="button"
                      onClick={() => void remove(row.id)}
                      disabled={busy === row.id}
                    >
                      Remove
                    </button>
                  </>
                ) : null}
                {row.configured && !isDefault ? (
                  <button
                    className="link-button"
                    type="button"
                    onClick={() => void makeDefault(row.id)}
                  >
                    Make default
                  </button>
                ) : null}
              </div>
            </div>
          );
        })}
      </div>

      {rows && rows.length > 0 ? (
        <div className="mcp-foot">
          <span className="hr-meta">
            <Icon name="checkCircle" size="xs" /> {configured.length} of {rows.length} configured
          </span>
        </div>
      ) : null}
    </div>
  );
}
