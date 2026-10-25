import type { GitWorktree } from "@grove/contract";
import { useEffect, useMemo, useRef, useState } from "react";
import { useBridge } from "../ipc/context";
import { errorText } from "../ipc/errors";
import { useSessionStore } from "../state/sessionStore";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore } from "../state/workspaceStore";
import {
  availableProviderIds,
  defaultModelFor,
  firstAvailableProvider,
  providerMeta,
  modelGroupsFor,
  providerForModel,
} from "../workspace/models";
import { Icon } from "../workspace/Icon";
import { basename, dirname } from "../workspace/tree";
import { Dialog } from "./Dialog";

function slug(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40);
}

export function NewSessionDialog() {
  const bridge = useBridge();
  const target = useUiStore((state) => state.newSession);
  const close = useUiStore((state) => state.closeNewSession);
  const notify = useUiStore((state) => state.notify);
  const projects = useWorkspaceStore((state) => state.projects);
  const worktrees = useWorkspaceStore((state) => state.worktrees);
  const prefs = useWorkspaceStore((state) => state.prefs);
  const refreshProjectGit = useWorkspaceStore((state) => state.refreshProjectGit);
  const create = useSessionStore((state) => state.create);
  const busy = useSessionStore((state) => state.busy);
  const storeError = useSessionStore((state) => state.error);
  const registered = useSessionStore((state) => state.providers);
  const providerStatuses = useSessionStore((state) => state.providerStatuses);

  const [title, setTitle] = useState("");
  const [projectId, setProjectId] = useState<string>("chat");
  const [originWorktree, setOriginWorktree] = useState<string | null>(null);
  const [useWorktree, setUseWorktree] = useState(false);
  const [branch, setBranch] = useState("");
  const [branchDirty, setBranchDirty] = useState(false);
  const [base, setBase] = useState("main");
  const [model, setModel] = useState(prefs.defaultModel);
  const [root, setRoot] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [working, setWorking] = useState(false);

  const open = target !== null;
  const project = projectId === "chat" ? null : (projects.find((entry) => entry.id === projectId) ?? null);
  const projectWorktrees: GitWorktree[] = project ? (worktrees[project.id] ?? []) : [];
  const isGit = Boolean(project?.isGit);
  const primary = projectWorktrees[0] ?? null;
  const originBranch =
    projectWorktrees.find((worktree) => worktree.path === originWorktree)?.branch ??
    (originWorktree ? basename(originWorktree) : null);

  const available = useMemo(
    () => availableProviderIds(registered, providerStatuses),
    [providerStatuses, registered],
  );
  // The catalogue's own model lists, so a model the presentation map does not know is still
  // attributed to the provider that offers it.
  const modelsByProvider = useMemo(
    () => Object.fromEntries(registered.map((spec) => [spec.id, spec.models])),
    [registered],
  );
  const modelGroups = useMemo(
    () => modelGroupsFor(prefs.enabledProviders, available, modelsByProvider),
    [available, modelsByProvider, prefs.enabledProviders],
  );
  const fallbackProvider = firstAvailableProvider(prefs.enabledProviders, available, prefs.defaultProvider);
  const initialized = useRef(false);

  useEffect(() => {
    if (!open) {
      initialized.current = false;
      return;
    }
    if (initialized.current || !target) return;
    initialized.current = true;
    const requestedProject =
      target.projectId === undefined
        ? (projects[0] ?? null)
        : (projects.find((entry) => entry.id === target.projectId) ?? null);
    const requestedWorktree = target.worktreePath ?? null;
    const requestedWorktrees = requestedProject ? (worktrees[requestedProject.id] ?? []) : [];
    const found =
      requestedProject && requestedWorktree
        ? requestedWorktrees.find((worktree) => {
            const path = worktree.path.replace(/\\/g, "/").toLowerCase().replace(/\/+$/, "");
            const wanted = requestedWorktree.replace(/\\/g, "/").toLowerCase().replace(/\/+$/, "");
            return wanted === path || wanted.startsWith(`${path}/`);
          })
        : null;
    setTitle("");
    setProjectId(requestedProject?.id ?? "chat");
    setOriginWorktree(found?.path ?? null);
    setUseWorktree(Boolean(requestedProject?.isGit) && !found);
    setBranch("");
    setBranchDirty(false);
    setBase(
      found?.branch ??
        requestedWorktrees[0]?.branch ??
        requestedProject?.defaultBranch ??
        "main",
    );
    const preferred = prefs.defaultModel;
    const preferredProvider = providerForModel(preferred, modelGroups);
    const usable = (id: string | null) =>
      Boolean(id) &&
      (available === undefined || available.includes(id as string)) &&
      prefs.enabledProviders[id as string] !== false;
    setModel(
      usable(preferredProvider)
        ? preferred
        : defaultModelFor(fallbackProvider, modelsByProvider[fallbackProvider]),
    );
    useSessionStore.setState({ error: null });
    setError(null);
    setWorking(false);
  }, [
    available,
    fallbackProvider,
    open,
    prefs.defaultModel,
    prefs.enabledProviders,
    projects,
    target,
    worktrees,
  ]);

  useEffect(() => {
    if (!open || !project || !isGit) {
      setRoot("");
      return;
    }
    let cancelled = false;
    void (async () => {
      if (prefs.worktreeRoot) {
        try {
          const listing = await bridge.invoke("fs_list_dirs", { path: prefs.worktreeRoot });
          if (!cancelled) setRoot(listing.path);
          return;
        } catch {
          // Fall through to the project's parent.
        }
      }
      if (!cancelled) setRoot(dirname(project.path) || project.path);
    })();
    return () => {
      cancelled = true;
    };
  }, [bridge, isGit, open, prefs.worktreeRoot, project]);

  useEffect(() => {
    if (!useWorktree || !project) return;
    if (!branchDirty) setBranch(`${project.name}/${slug(title) || "session"}`);
  }, [branchDirty, project, title, useWorktree]);

  const branchTail = slug(branch.split("/").pop() ?? "") || "session";
  const previewRoot = root || (project ? dirname(project.path) : "");
  const previewPath = useWorktree
    ? `${previewRoot.replace(/\/+$/, "")}/${project?.name ?? "project"}-wt/${branchTail}`
    : originWorktree ?? primary?.path ?? project?.path ?? "";

  const selectProject = (value: string) => {
    setProjectId(value);
    const next = value === "chat" ? null : projects.find((entry) => entry.id === value) ?? null;
    const nextWorktrees = next ? (worktrees[next.id] ?? []) : [];
    setOriginWorktree(null);
    setUseWorktree(Boolean(next?.isGit));
    setBase(nextWorktrees[0]?.branch ?? next?.defaultBranch ?? "main");
    setBranch("");
    setBranchDirty(false);
    setError(null);
  };

  const submit = async () => {
    if (!title.trim()) {
      setError("Give the session a name so you can find it in the tree.");
      return;
    }
    if (useWorktree && !branch.trim()) {
      setError("A worktree needs a branch name.");
      return;
    }
    if (useWorktree && branch.trim().startsWith("-")) {
      setError("A branch name must not start with '-'.");
      return;
    }
    const provider = providerForModel(model, modelGroups) ?? fallbackProvider;
    setWorking(true);
    setError(null);
    try {
      let worktreePath: string | null = null;
      if (project && isGit) {
        if (useWorktree) {
          const created = await bridge.invoke("git_worktree_create", {
            repoPath: project.path,
            branch: branch.trim(),
            base: base || project.defaultBranch || "main",
            path: previewPath,
          });
          worktreePath = created.path;
        } else {
          worktreePath = originWorktree ?? primary?.path ?? project.path;
        }
      }
      const created = await create(bridge, {
        provider,
        model,
        title: title.trim(),
        projectId: project?.id ?? null,
        worktreePath,
        config: { thinkingLevel: prefs.thinkingLevel },
      });
      if (!created) {
        setError(useSessionStore.getState().error ?? "The session could not be created.");
        return;
      }
      if (project) await refreshProjectGit(bridge, project);
      close();
      notify(`Session started on ${providerMeta(provider).name}`);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setWorking(false);
    }
  };

  const baseOptions = useMemo(() => {
    const values = [
      project?.defaultBranch ?? "main",
      ...(projectWorktrees.map((worktree) => worktree.branch).filter(Boolean) as string[]),
    ];
    return Array.from(new Set(values));
  }, [project?.defaultBranch, projectWorktrees]);

  const currentModelKnown = modelGroups.some((group) => group.models.includes(model));
  const noProvider = modelGroups.length === 0;

  return (
    <Dialog open={open} labelledBy="nd-title" onClose={close}>
      <div className="dlg-head">
        <h2 id="nd-title">New session</h2>
        <button className="icon-btn sm" type="button" aria-label="Close dialog" onClick={close}>
          <Icon name="x" size="sm" />
        </button>
      </div>
      <p className="dlg-sub">
        Each session gets its own model and transcript. The provider comes from the model you pick.
      </p>

      <div className="field">
        <label htmlFor="nd-task">What should this session work on?</label>
        <input
          id="nd-task"
          type="text"
          placeholder="e.g. Add a worktree picker to the new-session flow"
          autoComplete="off"
          autoFocus
          value={title}
          onChange={(event) => {
            setTitle(event.target.value);
            setError(null);
          }}
        />
        {error && !title.trim() ? (
          <p className="field-err" role="alert">
            {error}
          </p>
        ) : null}
      </div>

      <div className="field">
        <label htmlFor="nd-project">Project</label>
        <select id="nd-project" value={projectId} onChange={(event) => selectProject(event.target.value)}>
          <option value="chat">Chat — no project</option>
          {projects.map((entry) => (
            <option value={entry.id} key={entry.id}>
              {entry.name}
            </option>
          ))}
        </select>
      </div>

      {project && isGit ? (
        <div className="wt-block">
          <label className="switch-row" htmlFor="nd-wt">
            <input
              className="switch-input"
              type="checkbox"
              id="nd-wt"
              checked={useWorktree}
              onChange={(event) => {
                setUseWorktree(event.target.checked);
                setError(null);
              }}
            />
            <span className="switch" aria-hidden="true" />
            <span className="switch-copy">
              <span className="switch-title">
                {originWorktree ? "Create a new worktree from this branch" : "Create a worktree"}
              </span>
              <span className="switch-sub">
                {originWorktree
                  ? `Off: this session joins ${originBranch ?? "this checkout"} and shares its uncommitted changes.`
                  : "A separate checkout on its own branch, so this session cannot disturb your current one."}
              </span>
            </span>
          </label>

          {useWorktree ? (
            <div className="wt-detail">
              <div className="field-tight">
                <label htmlFor="nd-branch">Branch name</label>
                <input
                  id="nd-branch"
                  className="mono-input"
                  type="text"
                  autoComplete="off"
                  spellCheck={false}
                  value={branch}
                  onChange={(event) => {
                    setBranchDirty(true);
                    setBranch(event.target.value);
                    setError(null);
                  }}
                />
                {error && useWorktree && !branch.trim() ? (
                  <p className="field-err" role="alert">
                    {error}
                  </p>
                ) : null}
              </div>
              <div className="grid-2">
                <div className="field-tight">
                  <label htmlFor="nd-base">Base branch</label>
                  <select id="nd-base" value={base} onChange={(event) => setBase(event.target.value)}>
                    {baseOptions.map((value) => (
                      <option value={value} key={value}>
                        {value}
                      </option>
                    ))}
                  </select>
                </div>
                <div className="field-tight">
                  <span className="field-label">Checkout path</span>
                  <p className="path-preview" id="nd-path">
                    {previewPath}
                  </p>
                </div>
              </div>
            </div>
          ) : (
            <p className="wt-warn">
              This session runs in <span className="mono">{previewPath}</span> and shares uncommitted
              changes with every other session in that checkout.
            </p>
          )}
        </div>
      ) : project ? (
        <p className="git-none">
          No Git repository in {project.path}. This session runs in that folder — no worktree, no
          branch, and nothing to diff.
        </p>
      ) : null}

      <div className="field">
        <label htmlFor="nd-model">Model</label>
        <select id="nd-model" value={model} onChange={(event) => setModel(event.target.value)}>
          {!currentModelKnown ? <option value={model}>{model}</option> : null}
          {modelGroups.map(({ provider, models }) => (
            <optgroup label={provider.name} key={provider.id}>
              {models.map((value) => (
                <option value={value} key={value}>
                  {value}
                </option>
              ))}
            </optgroup>
          ))}
        </select>
        <p className="field-hint" id="nd-model-hint">
          {noProvider
            ? "Add a provider in Settings and paste its API key first."
            : `Runs on ${providerMeta(providerForModel(model, modelGroups) ?? fallbackProvider).name}.`}
        </p>
      </div>

      {storeError ? <p className="field-err">{storeError}</p> : null}
      {error && title.trim() ? (
        <p className="field-err" role="alert">
          {error}
        </p>
      ) : null}

      <div className="dlg-foot">
        <button className="btn-ghost" type="button" onClick={close}>
          Cancel
        </button>
        <button
          className="btn-primary"
          type="button"
          disabled={busy || working || noProvider}
          onClick={() => void submit()}
        >
          {working ? "Starting…" : "Start session"}
        </button>
      </div>
    </Dialog>
  );
}
