import { useEffect, useMemo, useState } from "react";
import { useBridge } from "../ipc/context";
import { useSessionStore } from "../state/sessionStore";
import { errorText } from "../ipc/errors";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore, type PermissionMode } from "../state/workspaceStore";
import type { ThinkingLevel } from "@grove/contract";
import {
  availableProviderIds,
  firstAvailableProvider,
  providerMeta,
  modelGroupsFor,
  providerForModel,
} from "../workspace/models";
import {
  knownThinkingLevels,
  nearestThinkingLevel,
  thinkingGroups,
  thinkingLevelsFor,
} from "../workspace/thinking";
import { Combo } from "../shell/Combo";

const PERMISSION_OPTIONS: { value: PermissionMode; label: string }[] = [
  { value: "ask", label: "Ask before running" },
  { value: "accept-edits", label: "Accept edits, ask for commands" },
  { value: "bypass", label: "Bypass approvals" },
];

export function GeneralPane({ onSaved }: { onSaved: () => void }) {
  const bridge = useBridge();
  const prefs = useWorkspaceStore((state) => state.prefs);
  const registered = useSessionStore((state) => state.providers);
  const providerStatuses = useSessionStore((state) => state.providerStatuses);
  const modelThinking = useSessionStore((state) => state.modelThinking);
  const saveSettings = useWorkspaceStore((state) => state.saveSettings);
  const notify = useUiStore((state) => state.notify);

  const [model, setModel] = useState(prefs.defaultModel);
  const [thinking, setThinking] = useState<ThinkingLevel>(prefs.thinkingLevel);
  const [permission, setPermission] = useState<PermissionMode>(prefs.permissionMode);
  const [root, setRoot] = useState(prefs.worktreeRoot);
  const [restore, setRestore] = useState(prefs.restoreSessions);
  const [error, setError] = useState<string | null>(null);
  const [working, setWorking] = useState(false);

  useEffect(() => {
    setModel(prefs.defaultModel);
    setThinking(prefs.thinkingLevel);
    setPermission(prefs.permissionMode);
    setRoot(prefs.worktreeRoot);
    setRestore(prefs.restoreSessions);
  }, [prefs]);

  const available = availableProviderIds(registered, providerStatuses);
  const modelsByProvider = Object.fromEntries(registered.map((spec) => [spec.id, spec.models]));
  const groups = useMemo(
    () => modelGroupsFor(prefs.enabledProviders, available, modelsByProvider),
    [available, modelsByProvider, prefs.enabledProviders],
  );
  const fallbackProvider = firstAvailableProvider(prefs.enabledProviders, available, prefs.defaultProvider);
  const selectedProvider = providerForModel(model, groups) ?? fallbackProvider;
  const knownLevels = useMemo(
    () => knownThinkingLevels(selectedProvider, model, modelThinking),
    [model, modelThinking, selectedProvider],
  );
  const thinkingLevels = knownLevels ?? thinkingLevelsFor(selectedProvider, model, modelThinking);

  // Keep the default thinking level valid when the default model changes. Only rewrite once the
  // catalogue knows the model; an unavailable catalogue must not silently downgrade the preference.
  useEffect(() => {
    if (!knownLevels) return;
    setThinking((current) =>
      knownLevels.includes(current) ? current : nearestThinkingLevel(current, knownLevels),
    );
  }, [knownLevels]);

  const previewRoot = root.trim().replace(/\/+$/, "") || "~/code";

  const save = async () => {
    const trimmed = root.trim();
    if (trimmed && !/^(~[/\\]|[A-Za-z]:[/\\]|\/)/.test(trimmed)) {
      setError("Worktree root needs a path, for example ~/code.");
      return;
    }
    setWorking(true);
    setError(null);
    try {
      await saveSettings(bridge, {
        defaultModel: model,
        thinkingLevel: thinking,
        permissionMode: permission,
        worktreeRoot: trimmed,
        restoreSessions: restore,
      });
      notify(`Settings saved — new sessions start on ${model}`);
      onSaved();
    } catch (err) {
      setError(errorText(err));
    } finally {
      setWorking(false);
    }
  };

  return (
    <>
      <div className="sm-pane-scroll">
        <div className="sm-pane-head">
          <h3>General</h3>
          <p>
            Defaults for new sessions. Sessions already running keep the provider and model they
            started with.
          </p>
        </div>
        <div className="sm-form">
          <div className="field">
            <label htmlFor="set-model">Default model</label>
            <Combo
              id="set-model"
              label="Default model"
              value={model}
              groups={groups.map(({ provider, models }) => ({
                label: provider.name,
                options: models.map((value) => ({ value })),
              }))}
              searchable
              onChange={setModel}
            />
            <p className="field-hint">
              Runs on {providerMeta(providerForModel(model, groups) ?? fallbackProvider).name}.
            </p>
          </div>
          <div className="field">
            <label htmlFor="set-thinking">Thinking level</label>
            <Combo
              id="set-thinking"
              label="Thinking level"
              value={thinking}
              groups={thinkingGroups(thinkingLevels)}
              onChange={(value) => setThinking(value as ThinkingLevel)}
            />
            <p className="field-hint">
              How much hidden reasoning to ask for on models that support it.
            </p>
          </div>
          <div className="field">
            <label htmlFor="set-perm">Permission mode</label>
            <select
              id="set-perm"
              value={permission}
              onChange={(event) => setPermission(event.target.value as PermissionMode)}
            >
              {PERMISSION_OPTIONS.map((option) => (
                <option value={option.value} key={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </div>
          <div className="field">
            <label htmlFor="set-wtroot">Worktree root</label>
            <input
              id="set-wtroot"
              className="mono-input"
              type="text"
              placeholder="~/code"
              autoComplete="off"
              spellCheck={false}
              value={root}
              onChange={(event) => {
                setRoot(event.target.value);
                setError(null);
              }}
            />
            <p className="field-hint">
              {root.trim()
                ? <>New worktrees land in <span className="mono">{previewRoot}/&lt;project&gt;-wt/&lt;branch&gt;</span>.</>
                : "Leave blank to create worktrees next to the project."}
            </p>
          </div>
          <label className="switch-row" htmlFor="set-restore">
            <input
              className="switch-input"
              type="checkbox"
              id="set-restore"
              checked={restore}
              onChange={(event) => setRestore(event.target.checked)}
            />
            <span className="switch" aria-hidden="true" />
            <span className="switch-copy">
              <span className="switch-title">Restore sessions on launch</span>
              <span className="switch-sub">
                Reopen the last selected session and its transcript when Grove starts.
              </span>
            </span>
          </label>
          {error ? (
            <p className="field-err" role="alert">
              {error}
            </p>
          ) : null}
        </div>
      </div>
      <div className="sm-foot">
        <button className="btn-ghost" type="button" onClick={onSaved}>
          Cancel
        </button>
        <button className="btn-primary" type="button" disabled={working} onClick={() => void save()}>
          {working ? "Saving…" : "Save changes"}
        </button>
      </div>
    </>
  );
}
