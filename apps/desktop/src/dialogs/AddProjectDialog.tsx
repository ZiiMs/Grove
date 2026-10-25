import type { DirListing, ProjectDetection } from "@grove/contract";
import { useEffect, useState } from "react";
import { useBridge } from "../ipc/context";
import { errorText } from "../ipc/errors";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore } from "../state/workspaceStore";
import { Icon } from "../workspace/Icon";
import { basename } from "../workspace/tree";
import { Dialog } from "./Dialog";

/** Add a folder as a project: pick it, detect git, name it, save. */
export function AddProjectDialog() {
  const bridge = useBridge();
  const open = useUiStore((state) => state.addProjectOpen);
  const close = useUiStore((state) => state.closeAddProject);
  const notify = useUiStore((state) => state.notify);
  const addProject = useWorkspaceStore((state) => state.addProject);

  const [mode, setMode] = useState<"form" | "picker">("form");
  const [path, setPath] = useState("");
  const [name, setName] = useState("");
  const [nameDirty, setNameDirty] = useState(false);
  const [detection, setDetection] = useState<ProjectDetection | null>(null);
  const [branch, setBranch] = useState("");
  const [remote, setRemote] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [working, setWorking] = useState(false);

  const [listing, setListing] = useState<DirListing | null>(null);
  const [pickerLoading, setPickerLoading] = useState(false);
  const [pickerError, setPickerError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setMode("form");
    setPath("");
    setName("");
    setNameDirty(false);
    setDetection(null);
    setBranch("");
    setRemote("");
    setError(null);
    setWorking(false);
    setListing(null);
    setPickerError(null);
  }, [open]);

  const loadDirs = async (target?: string) => {
    setPickerLoading(true);
    setPickerError(null);
    try {
      const next = await bridge.invoke("fs_list_dirs", target ? { path: target } : {});
      setListing(next);
    } catch (err) {
      setPickerError(errorText(err));
    } finally {
      setPickerLoading(false);
    }
  };

  const openPicker = () => {
    setMode("picker");
    void loadDirs(listing?.path);
  };

  const chooseFolder = async (folder: string) => {
    setPath(folder);
    setMode("form");
    setError(null);
    try {
      const next = await bridge.invoke("project_detect", { path: folder });
      setDetection(next);
      if (!nameDirty) setName(basename(next.path));
      setBranch(next.defaultBranch ?? "");
      setRemote(next.remote ?? "");
    } catch (err) {
      setDetection(null);
      setError(errorText(err));
    }
  };

  const submit = async () => {
    if (!path.trim()) {
      setError("Choose a folder first.");
      return;
    }
    if (!name.trim()) {
      setError("Give the project a name.");
      return;
    }
    setWorking(true);
    setError(null);
    try {
      const project = await addProject(bridge, {
        path,
        name: name.trim(),
        defaultBranch: detection?.isGit ? branch || null : null,
        remote: remote.trim() || null,
      });
      close();
      notify(`${project.name} added`);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setWorking(false);
    }
  };

  return (
    <Dialog
      open={open}
      labelledBy={mode === "form" ? "ap-title" : "ap-picker-title"}
      onClose={close}
    >
      {mode === "form" ? (
        <div id="ap-form">
          <div className="dlg-head">
            <h2 id="ap-title">Add project</h2>
            <button className="icon-btn sm" type="button" aria-label="Close dialog" onClick={close}>
              <Icon name="x" size="sm" />
            </button>
          </div>
          <p className="dlg-sub">
            Choose a folder on this machine. Grove takes the name from it and, when the folder is a
            Git repository, reads its branches.
          </p>

          <div className="field">
            <span className="field-label">Folder</span>
            <div className="path-row">
              <p className="path-preview">{path || "No folder selected yet"}</p>
              <button className="btn-ghost sm" type="button" onClick={openPicker}>
                Choose folder
              </button>
            </div>
          </div>

          <div className="field">
            <label htmlFor="ap-name">Name</label>
            <input
              id="ap-name"
              type="text"
              placeholder="grove"
              autoComplete="off"
              value={name}
              onChange={(event) => {
                setName(event.target.value);
                setNameDirty(true);
                setError(null);
              }}
            />
            <p className="field-hint">
              Takes the folder name automatically — edit it if you want a different label.
            </p>
          </div>

          {detection?.isGit ? (
            <div className="git-detect">
              <p className="git-detect-head">
                <Icon name="branch" size="sm" />
                Git repository detected
              </p>
              <div className="grid-2">
                <div className="field-tight">
                  <label htmlFor="ap-branch">Default branch</label>
                  <select
                    id="ap-branch"
                    value={branch}
                    onChange={(event) => setBranch(event.target.value)}
                  >
                    {(detection.branches.length ? detection.branches : [branch || "main"]).map(
                      (value) => (
                        <option value={value} key={value}>
                          {value}
                        </option>
                      ),
                    )}
                  </select>
                </div>
                <div className="field-tight">
                  <label htmlFor="ap-remote">
                    Remote <span className="opt">optional</span>
                  </label>
                  <input
                    id="ap-remote"
                    className="mono-input"
                    type="text"
                    placeholder="org/repo"
                    autoComplete="off"
                    spellCheck={false}
                    value={remote}
                    onChange={(event) => setRemote(event.target.value)}
                  />
                </div>
              </div>
              <p className="field-hint">
                {detection.currentBranch
                  ? `Current branch: ${detection.currentBranch}. `
                  : ""}
                New sessions branch from the default branch.
              </p>
            </div>
          ) : path ? (
            <p className="git-none">
              No Git repository in this folder. It is added as a plain workspace: sessions run in the
              folder with no branches, worktrees or diffs.
            </p>
          ) : null}

          {error ? (
            <p className="field-err" role="alert">
              {error}
            </p>
          ) : null}

          <div className="dlg-foot">
            <button className="btn-ghost" type="button" onClick={close}>
              Cancel
            </button>
            <button className="btn-primary" type="button" disabled={working} onClick={() => void submit()}>
              {working ? "Adding…" : "Add project"}
            </button>
          </div>
        </div>
      ) : (
        <div id="ap-picker">
          <div className="dlg-head">
            <h2 id="ap-picker-title">Choose a folder</h2>
            <button
              className="icon-btn sm"
              type="button"
              aria-label="Back to new project"
              onClick={() => setMode("form")}
            >
              <Icon name="x" size="sm" />
            </button>
          </div>
          <p className="dlg-sub">Open folders by clicking them. Use this folder selects the one you are in.</p>
          <div className="fp-bar">
            <button
              className="icon-btn sm"
              type="button"
              aria-label="Go up one folder"
              disabled={!listing?.parent}
              onClick={() => listing?.parent && void loadDirs(listing.parent)}
            >
              <Icon name="chevronLeft" size="sm" />
            </button>
            <p className="fp-path">{listing?.path ?? "…"}</p>
          </div>
          <div className="fp-list">
            {pickerLoading ? (
              <p className="git-empty">Reading folders…</p>
            ) : pickerError ? (
              <p className="git-empty">{pickerError}</p>
            ) : listing?.dirs.length ? (
              listing.dirs.map((dir) => (
                <button
                  className="fp-row"
                  type="button"
                  key={dir.path}
                  onClick={() => void loadDirs(dir.path)}
                >
                  <Icon name="folder" size="sm" />
                  <span className="fp-name">{dir.name}</span>
                </button>
              ))
            ) : (
              <p className="git-empty">No subfolders here.</p>
            )}
          </div>
          <div className="dlg-foot">
            <button className="btn-ghost" type="button" onClick={() => setMode("form")}>
              Back
            </button>
            <button
              className="btn-primary"
              type="button"
              disabled={!listing || pickerLoading}
              onClick={() => listing && void chooseFolder(listing.path)}
            >
              Use this folder
            </button>
          </div>
        </div>
      )}
    </Dialog>
  );
}
