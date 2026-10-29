import type { DirEntry, DirListing, FsRoots, ProjectDetection } from "@grove/contract";
import { Fragment, useEffect, useRef, useState, type KeyboardEvent } from "react";
import { useBridge } from "../ipc/context";
import { errorText } from "../ipc/errors";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore } from "../state/workspaceStore";
import { Icon } from "../workspace/Icon";
import { basename, normalizeFolderPath } from "../workspace/tree";
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

  const [roots, setRoots] = useState<FsRoots | null>(null);
  const [listing, setListing] = useState<DirListing | null>(null);
  const [address, setAddress] = useState("");
  const [pickerLoading, setPickerLoading] = useState(false);
  const [pickerError, setPickerError] = useState<string | null>(null);
  const [newFolderOpen, setNewFolderOpen] = useState(false);
  const [newFolderName, setNewFolderName] = useState("");
  const [newFolderError, setNewFolderError] = useState<string | null>(null);
  const [creatingFolder, setCreatingFolder] = useState(false);
  const [activeIndex, setActiveIndex] = useState(-1);

  const addressRef = useRef<HTMLInputElement>(null);
  const newFolderRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const rowRefs = useRef<Array<HTMLButtonElement | null>>([]);

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
    setRoots(null);
    setListing(null);
    setAddress("");
    setPickerError(null);
    setNewFolderOpen(false);
    setNewFolderName("");
    setNewFolderError(null);
    setCreatingFolder(false);
    setActiveIndex(-1);
  }, [open]);

  useEffect(() => {
    if (mode === "picker") addressRef.current?.focus();
  }, [mode]);

  useEffect(() => {
    if (newFolderOpen) newFolderRef.current?.focus();
  }, [newFolderOpen]);

  const loadRoots = async () => {
    try {
      setRoots(await bridge.invoke("fs_list_roots", {}));
    } catch {
      // Roots are shortcuts; a failure must not stop the picker from browsing folders.
    }
  };

  const loadDirs = async (target?: string, focus: "address" | "list" = "address") => {
    setPickerLoading(true);
    setPickerError(null);
    setNewFolderError(null);
    try {
      const next = await bridge.invoke("fs_list_dirs", target ? { path: target } : {});
      setListing(next);
      setAddress(next.path);
      setActiveIndex(-1);
      if (focus === "list") listRef.current?.focus();
      else addressRef.current?.focus();
    } catch (err) {
      // Keep the current listing and whatever the user typed; the error sits under the bar.
      setPickerError(errorText(err));
    } finally {
      setPickerLoading(false);
    }
  };

  const openPicker = () => {
    setMode("picker");
    setPickerError(null);
    setNewFolderOpen(false);
    setNewFolderName("");
    setNewFolderError(null);
    void loadRoots();
    void loadDirs(listing?.path);
  };

  const navigate = (raw: string, focus: "address" | "list" = "address") => {
    const target = normalizeFolderPath(raw);
    if (!target) {
      setPickerError("Enter a folder path.");
      return;
    }
    void loadDirs(target, focus);
  };

  const createFolder = async () => {
    if (!listing || creatingFolder) return;
    setCreatingFolder(true);
    setNewFolderError(null);
    try {
      const entry: DirEntry = await bridge.invoke("fs_create_dir", {
        parent: listing.path,
        name: newFolderName,
      });
      setNewFolderOpen(false);
      setNewFolderName("");
      await loadDirs(entry.path, "list");
    } catch (err) {
      setNewFolderError(errorText(err));
    } finally {
      setCreatingFolder(false);
    }
  };

  const handleListKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const dirs = listing?.dirs ?? [];
    if (!dirs.length) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      const next = Math.min(activeIndex + 1, dirs.length - 1);
      setActiveIndex(next);
      rowRefs.current[next]?.focus();
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      const next = Math.max(activeIndex - 1, 0);
      setActiveIndex(next);
      rowRefs.current[next]?.focus();
    } else if (event.key === "Enter" && activeIndex >= 0) {
      event.preventDefault();
      void loadDirs(dirs[activeIndex].path, "list");
    } else if (event.key === "Backspace" && listing?.parent) {
      event.preventDefault();
      void loadDirs(listing.parent, "list");
    }
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

  const activeRoot = (roots?.roots ?? [])
    .filter((root) => rootContains(root.path, listing?.path ?? ""))
    .sort((left, right) => right.path.length - left.path.length)[0]?.path;
  const crumbs = breadcrumbs(listing?.path ?? "", roots?.home ?? null);

  return (
    <Dialog
      open={open}
      className="add-project-dialog"
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
                {detection.currentBranch ? `Current branch: ${detection.currentBranch}. ` : ""}
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
            <button className="icon-btn sm" type="button" aria-label="Close dialog" onClick={close}>
              <Icon name="x" size="sm" />
            </button>
          </div>
          <p className="dlg-sub">
            Paste a path and press Enter, or open folders by clicking them. Select folder uses the
            folder you are in.
          </p>

          <div className="fp-address-row">
            <button
              className="icon-btn sm"
              type="button"
              aria-label="Go up one folder"
              disabled={!listing?.parent || pickerLoading}
              onClick={() => listing?.parent && void loadDirs(listing.parent)}
            >
              <Icon name="chevronLeft" size="sm" />
            </button>
            <input
              ref={addressRef}
              className="fp-address"
              type="text"
              aria-label="Folder path"
              data-autofocus
              autoComplete="off"
              spellCheck={false}
              value={address}
              onChange={(event) => {
                setAddress(event.target.value);
                setPickerError(null);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  navigate(address);
                } else if (event.key === "Escape") {
                  setAddress(listing?.path ?? "");
                  setPickerError(null);
                }
              }}
            />
            <button
              className="btn-ghost sm"
              type="button"
              aria-expanded={newFolderOpen}
              disabled={!listing || pickerLoading}
              onClick={() => {
                setNewFolderOpen((wasOpen) => !wasOpen);
                setNewFolderName("");
                setNewFolderError(null);
              }}
            >
              New folder
            </button>
          </div>

          {pickerError ? (
            <p className="field-err" role="alert">
              {pickerError}
            </p>
          ) : null}

          {newFolderOpen ? (
            <div className="fp-new-row">
              <input
                ref={newFolderRef}
                className="fp-address"
                type="text"
                aria-label="New folder name"
                placeholder="new-folder"
                autoComplete="off"
                spellCheck={false}
                value={newFolderName}
                onChange={(event) => {
                  setNewFolderName(event.target.value);
                  setNewFolderError(null);
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    void createFolder();
                  } else if (event.key === "Escape") {
                    event.preventDefault();
                    setNewFolderOpen(false);
                    setNewFolderError(null);
                  }
                }}
              />
              <button
                className="btn-primary sm"
                type="button"
                disabled={creatingFolder}
                onClick={() => void createFolder()}
              >
                {creatingFolder ? "Creating…" : "Create"}
              </button>
            </div>
          ) : null}

          {newFolderError ? (
            <p className="field-err" role="alert">
              {newFolderError}
            </p>
          ) : null}

          <nav className="fp-crumbs" aria-label="Folder path segments">
            {crumbs.map((crumb, index) => (
              <Fragment key={crumb.path}>
                {index > 0 ? <span className="fp-crumb-sep">/</span> : null}
                {index === crumbs.length - 1 ? (
                  <span className="fp-crumb" aria-current="page">
                    {crumb.label}
                  </span>
                ) : (
                  <button
                    className="fp-crumb"
                    type="button"
                    onClick={() => void loadDirs(crumb.path)}
                  >
                    {crumb.label}
                  </button>
                )}
              </Fragment>
            ))}
          </nav>

          {roots?.roots.length ? (
            <div className="fp-chips" role="group" aria-label="Folder roots">
              {roots.roots.map((root) => (
                <button
                  className="fp-chip"
                  type="button"
                  key={`${root.kind}:${root.path}`}
                  aria-label={root.kind === "home" ? "Home folder" : root.name}
                  aria-current={root.path === activeRoot ? "true" : undefined}
                  title={root.path}
                  onClick={() => void loadDirs(root.path)}
                >
                  {root.kind === "home" ? "~" : root.name}
                </button>
              ))}
            </div>
          ) : null}

          <div
            className="fp-list"
            role="listbox"
            aria-label="Folders"
            tabIndex={-1}
            ref={listRef}
            onKeyDown={handleListKeyDown}
          >
            {pickerLoading ? (
              <p className="git-empty">Reading folders…</p>
            ) : listing?.dirs.length ? (
              listing.dirs.map((dir, index) => (
                <button
                  ref={(node) => {
                    rowRefs.current[index] = node;
                  }}
                  className="fp-row"
                  type="button"
                  role="option"
                  aria-selected={index === activeIndex}
                  tabIndex={index === activeIndex ? 0 : -1}
                  key={dir.path}
                  onFocus={() => setActiveIndex(index)}
                  onClick={() => void loadDirs(dir.path, "list")}
                >
                  <Icon name="folder" size="sm" />
                  <span className="fp-name">{dir.name}</span>
                  <span className="chev">
                    <Icon name="chevronRight" size="sm" />
                  </span>
                </button>
              ))
            ) : (
              <p className="fp-empty">No subfolders here.</p>
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
              Select folder
            </button>
          </div>
        </div>
      )}
    </Dialog>
  );
}

/** One clickable segment per folder; the last one is where the picker already is. */
function breadcrumbs(path: string, home: string | null): Array<{ label: string; path: string }> {
  const clean = path.replace(/\/+$/, "") || path;
  if (!clean) return [];
  const homePath = home?.replace(/\/+$/, "");

  if (homePath && (clean === homePath || clean.startsWith(`${homePath}/`))) {
    let at = homePath;
    return [
      { label: "Home", path: homePath },
      ...clean
        .slice(homePath.length)
        .split("/")
        .filter(Boolean)
        .map((segment) => {
          at = `${at}/${segment}`;
          return { label: segment, path: at };
        }),
    ];
  }

  if (/^[A-Za-z]:/.test(clean)) {
    const drive = clean.slice(0, 2);
    let at = `${drive}/`;
    return [
      { label: drive, path: at },
      ...clean
        .slice(2)
        .split("/")
        .filter(Boolean)
        .map((segment) => {
          at = `${at.replace(/\/$/, "")}/${segment}`;
          return { label: segment, path: at };
        }),
    ];
  }

  let at = clean.startsWith("/") ? "/" : "";
  return clean
    .split("/")
    .filter(Boolean)
    .map((segment) => {
      at = at === "/" ? `/${segment}` : at ? `${at}/${segment}` : segment;
      return { label: segment, path: at };
    });
}

/** Whether a root chip covers the current folder; the longest match wins. */
function rootContains(rootPath: string, path: string): boolean {
  if (!path) return false;
  if (rootPath === path) return true;
  if (rootPath === "/") return path.startsWith("/");
  return path.startsWith(`${rootPath.replace(/\/+$/, "")}/`);
}
