import type { CatalogEntry, SkillTarget } from "@grove/contract";
import { useEffect, useMemo, useRef, useState } from "react";
import { useBridge } from "../ipc/context";
import { useSkillsStore } from "../state/skillsStore";
import { useUiStore } from "../state/uiStore";
import { Icon } from "../workspace/Icon";
import { SkillInstallDialog } from "./SkillInstallDialog";
import { SkillSourceDialog } from "./SkillSourceDialog";
import { SkillUninstallDialog } from "./SkillUninstallDialog";

function initials(source: string): string {
  const letters = source.replace(/[^a-z0-9]/gi, " ").trim().split(/\s+/);
  return letters
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? "")
    .join("") || "SK";
}

function firstParagraph(body: string): string {
  const text = body
    .split("\n")
    .filter((line) => !line.trim().startsWith("#"))
    .join(" ")
    .replace(/\s+/g, " ")
    .trim();
  return text.length > 240 ? `${text.slice(0, 237)}…` : text;
}

function bodyBullets(body: string): string[] {
  return body
    .split("\n")
    .map((line) => /^\s*[-*]\s+(.+)$/.exec(line)?.[1]?.trim() ?? "")
    .filter((line) => line.length > 0)
    .slice(0, 4);
}

function skillKey(name: string): string {
  return name.trim().toLowerCase();
}

const FALLBACK_TARGETS: SkillTarget[] = [
  { id: "user", label: "User .agents/skills", path: "~/.agents/skills", exists: false },
  { id: "agents", label: "Project .agents/skills", path: ".agents/skills", exists: false },
];

export function SkillsPane({ projectRoot }: { projectRoot?: string }) {
  const bridge = useBridge();
  const catalog = useSkillsStore((state) => state.catalog);
  const installs = useSkillsStore((state) => state.installs);
  const inventory = useSkillsStore((state) => state.inventory);
  const catalogSources = useSkillsStore((state) => state.sources);
  const targets = useSkillsStore((state) => state.targets);
  const preview = useSkillsStore((state) => state.preview);
  const selectedId = useSkillsStore((state) => state.selectedId);
  const query = useSkillsStore((state) => state.query);
  const loading = useSkillsStore((state) => state.loading);
  const previewLoading = useSkillsStore((state) => state.previewLoading);
  const installing = useSkillsStore((state) => state.installing);
  const refreshing = useSkillsStore((state) => state.refreshing);
  const error = useSkillsStore((state) => state.error);
  const load = useSkillsStore((state) => state.load);
  const select = useSkillsStore((state) => state.select);
  const setQuery = useSkillsStore((state) => state.setQuery);
  const install = useSkillsStore((state) => state.install);
  const remove = useSkillsStore((state) => state.remove);
  const addSource = useSkillsStore((state) => state.addSource);
  const removeAll = useSkillsStore((state) => state.removeAll);
  const notify = useUiStore((state) => state.notify);

  const loadedRoot = useRef<string | undefined>(undefined);
  const selectAllRef = useRef<HTMLInputElement>(null);
  const [catalogId, setCatalogId] = useState<string | null>(null);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [pendingInstall, setPendingInstall] = useState<CatalogEntry[] | null>(null);
  const [sourceOpen, setSourceOpen] = useState(false);
  const [sourceBusy, setSourceBusy] = useState(false);
  const [sourceError, setSourceError] = useState<string | null>(null);
  const [uninstallOpen, setUninstallOpen] = useState(false);
  const [removingAll, setRemovingAll] = useState(false);

  useEffect(() => {
    if (!catalog || loadedRoot.current !== projectRoot) {
      loadedRoot.current = projectRoot;
      void load(bridge, projectRoot);
    }
  }, [bridge, catalog, load, projectRoot]);

  const allEntries = catalog?.entries ?? [];
  const sources = useMemo(() => {
    const map = new Map<string, CatalogEntry[]>();
    for (const entry of allEntries) {
      const list = map.get(entry.source) ?? [];
      list.push(entry);
      map.set(entry.source, list);
    }
    const names = [...catalogSources];
    for (const source of map.keys()) {
      if (!names.includes(source)) names.push(source);
    }
    return names.map((source) => [source, map.get(source) ?? []] as [string, CatalogEntry[]]);
  }, [allEntries, catalogSources]);

  useEffect(() => {
    if (catalogId && sources.some(([source]) => source === catalogId)) return;
    setCatalogId(sources[0]?.[0] ?? null);
  }, [catalogId, sources]);

  // Drop selected skills that left the catalog, without widening the selection when the cache
  // reloads and `allEntries` is rebuilt.
  useEffect(() => {
    const known = new Set((catalog?.entries ?? []).map((entry) => entry.id));
    setSelectedIds((previous) => {
      const next = previous.filter((id) => known.has(id));
      return next.length === previous.length ? previous : next;
    });
  }, [catalog]);

  const searching = query.trim().length > 0;
  const visible = useMemo(() => {
    if (searching) {
      const needle = query.trim().toLowerCase();
      return allEntries.filter((entry) =>
        `${entry.name} ${entry.source} ${entry.path}`.toLowerCase().includes(needle),
      );
    }
    return sources.find(([source]) => source === catalogId)?.[1] ?? [];
  }, [allEntries, catalogId, query, searching, sources]);

  useEffect(() => {
    if (loading || visible.length === 0) return;
    if (!visible.some((entry) => entry.id === selectedId)) {
      select(bridge, visible[0]);
    }
  }, [bridge, loading, select, selectedId, visible]);

  const selectedSet = useMemo(() => new Set(selectedIds), [selectedIds]);
  const selectedEntries = useMemo(
    () => allEntries.filter((entry) => selectedSet.has(entry.id)),
    [allEntries, selectedSet],
  );
  const selectedVisible = visible.filter((entry) => selectedSet.has(entry.id)).length;
  const allVisibleSelected = visible.length > 0 && selectedVisible === visible.length;

  useEffect(() => {
    const node = selectAllRef.current;
    if (node) node.indeterminate = selectedVisible > 0 && !allVisibleSelected;
  }, [allVisibleSelected, selectedVisible]);

  const selected =
    visible.find((entry) => entry.id === selectedId) ??
    allEntries.find((entry) => entry.id === selectedId) ??
    null;
  const frontmatter = preview?.frontmatter ?? {};
  // Names that are present in either target, whether Grove installed them or not.
  const installedNames = useMemo(() => {
    const names = new Set<string>();
    for (const record of installs) names.add(skillKey(record.skill));
    for (const item of inventory) {
      if (!projectRoot && item.target === "agents") continue;
      names.add(skillKey(item.name));
    }
    return names;
  }, [installs, inventory, projectRoot]);
  const allPaths = useMemo(() => {
    const paths = new Set<string>();
    for (const item of inventory) {
      if (!projectRoot && item.target === "agents") continue;
      paths.add(item.path);
    }
    for (const record of installs) {
      if (!projectRoot && record.target === "agents") continue;
      paths.add(record.path);
    }
    return [...paths];
  }, [installs, inventory, projectRoot]);
  const installedRecords = selected
    ? installs.filter((record) => skillKey(record.skill) === skillKey(selected.name))
    : [];
  const detectedCopies = selected
    ? inventory.filter(
        (item) =>
          skillKey(item.name) === skillKey(selected.name) &&
          (projectRoot || item.target !== "agents"),
      )
    : [];
  const installedRecord = installedRecords[0];
  const detectedCopy = detectedCopies[0];
  const removable = installedRecord ?? detectedCopy ?? null;
  const isInstalled = Boolean(removable);
  const installedCount = installedNames.size;
  const agentsTarget = targets.find((target) => target.id === "agents");
  const installedTarget = installedRecord
    ? targets.find((target) => target.id === installedRecord.target)
    : detectedCopy
      ? targets.find((target) => target.id === detectedCopy.target)
      : undefined;
  const description =
    frontmatter.description ?? (preview ? firstParagraph(preview.body) : selected?.path ?? "");
  const adds = preview ? bodyBullets(preview.body) : [];
  const tags = (frontmatter.tags ?? "")
    .split(",")
    .map((tag) => tag.trim())
    .filter(Boolean);

  // A chat has no project of its own, so only the user-wide location is offered there.
  const installTargets = useMemo(() => {
    const available = targets.filter((target) => projectRoot || target.id !== "agents");
    if (available.length > 0) return available;
    return FALLBACK_TARGETS.filter((target) => projectRoot || target.id !== "agents");
  }, [projectRoot, targets]);

  const toggleSelected = (id: string) => {
    setSelectedIds((previous) =>
      previous.includes(id)
        ? previous.filter((entry) => entry !== id)
        : [...previous, id],
    );
  };

  const toggleAllVisible = () => {
    const visibleIds = new Set(visible.map((entry) => entry.id));
    setSelectedIds((previous) => {
      const next = new Set(previous);
      if (allVisibleSelected) {
        for (const id of visibleIds) next.delete(id);
      } else {
        for (const id of visibleIds) next.add(id);
      }
      return [...next];
    });
  };

  const openInstall = (entries: CatalogEntry[]) => {
    if (entries.length === 0 || installing !== null) return;
    setPendingInstall(entries);
  };

  const runInstall = async (targetId: string) => {
    const entries = pendingInstall;
    if (!entries || entries.length === 0) return;
    const target = installTargets.find((option) => option.id === targetId);
    if (!target) return;
    const result = await install(bridge, entries, target.id, projectRoot);
    setPendingInstall(null);
    if (result.error && result.installed === 0) {
      notify(result.error, "error");
      return;
    }
    const done = new Set(entries.slice(0, result.installed).map((entry) => entry.id));
    setSelectedIds((previous) => previous.filter((id) => !done.has(id)));
    if (result.error) {
      notify(`${result.installed} installed · ${result.error}`, "error");
    } else {
      notify(
        `${result.installed} skill${result.installed === 1 ? "" : "s"} installed to ${target.label}`,
      );
    }
  };

  const doRefresh = async () => {
    await load(bridge, projectRoot, true);
    const message = useSkillsStore.getState().error;
    if (message) notify(message, "error");
    else notify("Catalog refreshed");
  };

  const doAddSource = async (input: string) => {
    setSourceBusy(true);
    setSourceError(null);
    const result = await addSource(bridge, input);
    setSourceBusy(false);
    if (!result.ok) {
      setSourceError(result.error ?? "Could not add that catalog.");
      return;
    }
    setSourceOpen(false);
    setSelectedIds([]);
    setQuery("");
    setCatalogId(result.source);
    notify(`Catalog ${result.source} added`);
    if (result.error) notify(result.error, "error");
  };

  const doRemoveAll = async () => {
    setRemovingAll(true);
    const result = await removeAll(bridge, allPaths, projectRoot);
    setRemovingAll(false);
    setUninstallOpen(false);
    setSelectedIds([]);
    if (result.error) {
      notify(`${result.removed} removed · ${result.error}`, "error");
    } else {
      notify(
        `${result.removed} skill cop${result.removed === 1 ? "y" : "ies"} removed`,
      );
    }
  };

  const doRemove = async () => {
    if (!removable) return;
    const ok = await remove(bridge, removable.path, projectRoot);
    if (ok) notify(`${selected?.name ?? removable.path} removed`);
    else notify(useSkillsStore.getState().error ?? "Remove failed", "error");
  };

  const fallbackPath =
    (projectRoot ? agentsTarget : targets.find((target) => target.id === "user"))?.path ??
    ".agents/skills";
  const detailPath = installedRecord?.path
    ? `${installedRecord.path}/SKILL.md`
    : detectedCopy?.path
      ? `${detectedCopy.path}/SKILL.md`
      : `${fallbackPath}/${selected?.name ?? ""}/SKILL.md`;

  return (
    <>
      <div className="sk-head">
        <div className="sk-head-text">
          <h3>Skills</h3>
          <p>
            Pick a skill, read what it changes, then install it to your user or this project.
          </p>
        </div>
        <div className="sk-head-tools">
          <button
            className="icon-btn sm"
            type="button"
            aria-label="Refresh catalog"
            title="Refresh catalog"
            disabled={loading || refreshing}
            onClick={() => void doRefresh()}
          >
            <Icon
              name="refresh"
              size="sm"
              className={loading || refreshing ? "spin" : undefined}
            />
          </button>
          <span className="count-pill">{installedCount} installed</span>
          <button
            className="btn-ghost sm danger"
            type="button"
            disabled={allPaths.length === 0 || removingAll || installing !== null}
            title="Remove every installed skill"
            onClick={() => setUninstallOpen(true)}
          >
            <Icon name="trash" size="sm" />
            Uninstall all
          </button>
          <button
            className="btn-primary sm"
            type="button"
            disabled={selectedEntries.length === 0 || installing !== null}
            title="Install the selected skills"
            onClick={() => openInstall(selectedEntries)}
          >
            <Icon name="download" size="sm" />
            {installing !== null
              ? "Installing…"
              : `Install${selectedEntries.length > 0 ? ` ${selectedEntries.length}` : ""}`}
          </button>
        </div>
      </div>

      <div className="sk-browser">
        <div className="sk-catalogs od-scroll" role="group" aria-label="Skill catalogs">
          {sources.map(([source, entries]) => (
            <button
              className="sk-cat"
              type="button"
              key={source}
              aria-current={!searching && source === catalogId}
              onClick={() => {
                setQuery("");
                setSelectedIds([]);
                setCatalogId(source);
              }}
            >
              <span className="sk-cat-mark" aria-hidden="true">
                {initials(source)}
              </span>
              <span className="sk-cat-body">
                <span className="sk-cat-name">{source}</span>
                <span className="sk-cat-blurb">GitHub catalog</span>
              </span>
              <span className="sk-cat-count">{entries.length}</span>
            </button>
          ))}
          <button
            className="sk-cat sk-cat-add"
            type="button"
            onClick={() => {
              setSourceError(null);
              setSourceOpen(true);
            }}
          >
            <span className="sk-cat-mark" aria-hidden="true">
              <Icon name="plus" size="xs" />
            </span>
            <span className="sk-cat-body">
              <span className="sk-cat-name">Add catalog</span>
              <span className="sk-cat-blurb">From GitHub</span>
            </span>
          </button>
        </div>

        <div className="sk-list-col">
          <div className="sk-list-head">
            <div className="search sk-list-search">
              <span className="search-ico">
                <Icon name="search" size="sm" />
              </span>
              <label className="sr-only" htmlFor="sm-skill-search">
                Search all skills
              </label>
              <input
                id="sm-skill-search"
                type="search"
                placeholder="Search all skills"
                autoComplete="off"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </div>
            <label className="sk-select-all">
              <input
                ref={selectAllRef}
                className="sk-check"
                type="checkbox"
                checked={allVisibleSelected}
                disabled={visible.length === 0}
                onChange={toggleAllVisible}
                aria-label="Select all visible skills"
              />
              <span>Select all</span>
            </label>
            <span className="sk-list-count">
              {visible.length} skill{visible.length === 1 ? "" : "s"}
            </span>
          </div>
          <div className="sk-list od-scroll" role="group" aria-label="Skills">
            {loading ? (
              <p className="sk-detail-empty" style={{ padding: "16px 14px" }}>
                Reading the catalog…
              </p>
            ) : visible.length === 0 ? (
              <p className="sk-detail-empty" style={{ padding: "18px 14px" }}>
                {searching ? "No skills match that search." : "This catalog has no skills."}
              </p>
            ) : (
              visible.map((entry) => {
                const present = installedNames.has(skillKey(entry.name));
                return (
                  <div
                    className="sk-row"
                    key={entry.id}
                    aria-current={entry.id === selectedId}
                  >
                    <input
                      className="sk-check"
                      type="checkbox"
                      checked={selectedSet.has(entry.id)}
                      onChange={() => toggleSelected(entry.id)}
                      aria-label={`Select ${entry.name}`}
                    />
                    <button
                      className="sk-row-main"
                      type="button"
                      aria-current={entry.id === selectedId}
                      onClick={() => select(bridge, entry)}
                    >
                      <span className="sk-row-name">
                        <span className="n">{entry.name}</span>
                        {searching ? <span className="v">· {entry.source}</span> : null}
                      </span>
                      <span className="sk-row-desc">{entry.source}</span>
                    </button>
                    {installing === entry.id ? (
                      <span className="sk-state wait">installing…</span>
                    ) : present ? (
                      <span className="sk-state ok">
                        <Icon name="checkCircle" size="xs" />
                        installed
                      </span>
                    ) : null}
                  </div>
                );
              })
            )}
          </div>
        </div>

        <div className="sk-detail od-scroll" aria-label="Selected skill">
          {!selected ? (
            <p className="sk-detail-empty">Select a skill to see what it changes.</p>
          ) : previewLoading && !preview ? (
            <p className="sk-detail-empty">Reading {selected.name}…</p>
          ) : (
            <>
              <div className="sk-detail-head">
                <span className="sk-detail-ico">
                  <Icon name="layers" />
                </span>
                <div style={{ minWidth: 0 }}>
                  <div className="sk-detail-name">{selected.name}</div>
                  <div className="sk-detail-meta">
                    {selected.source}
                    {frontmatter.version ? ` · v${frontmatter.version}` : ""}
                    {frontmatter.author ? ` · ${frontmatter.author}` : ""}
                  </div>
                </div>
              </div>
              <p className="sk-detail-desc">{description}</p>
              {tags.length ? (
                <div className="sk-row-tags">
                  {tags.map((tag) => (
                    <span className="sk-tag" key={tag}>
                      {tag}
                    </span>
                  ))}
                </div>
              ) : null}
              <p className="sk-sub">What it changes</p>
              {adds.length ? (
                <ul className="sk-adds">
                  {adds.map((line) => (
                    <li key={line}>{line}</li>
                  ))}
                </ul>
              ) : (
                <p className="sk-hint">{preview ? firstParagraph(preview.body) : "Open the preview to read the skill."}</p>
              )}
              <p className="sk-sub">Files</p>
              <code className="sk-path">{detailPath}</code>
              <div className="sk-actions">
                <button
                  className="act-btn primary"
                  type="button"
                  aria-label={`Install ${selected.name}`}
                  title={isInstalled ? "Install to another location" : "Install skill"}
                  disabled={installing !== null}
                  onClick={() => openInstall([selected])}
                >
                  <Icon name="download" size="sm" />
                </button>
                {isInstalled ? (
                  <button
                    className="act-btn danger"
                    type="button"
                    aria-label={`Remove ${selected.name}`}
                    title="Remove skill"
                    onClick={() => void doRemove()}
                  >
                    <Icon name="trash" size="sm" />
                  </button>
                ) : null}
                {installedRecord ? (
                  <span className="sk-hint">
                    Installed in {installedTarget?.label ?? installedRecord.target}
                  </span>
                ) : detectedCopy ? (
                  <span className="sk-hint">
                    Found in {installedTarget?.label ?? detectedCopy.target}
                  </span>
                ) : null}
              </div>
              <p className="sk-hint">
                {installedRecord
                  ? "Installed. Remove deletes this copy."
                  : detectedCopy
                    ? "Found on disk. Remove deletes this folder."
                    : "Choose user or project when you install."}
              </p>
            </>
          )}
          {error ? <p className="field-err">{error}</p> : null}
          {catalog?.offline ? (
            <p className="sk-hint">Offline — showing the cached catalog from {catalog.fetchedAt ?? "an earlier run"}.</p>
          ) : null}
        </div>
      </div>

      <SkillInstallDialog
        entries={pendingInstall}
        targets={installTargets}
        preferredTarget={projectRoot ? "agents" : "user"}
        busy={installing !== null}
        onClose={() => setPendingInstall(null)}
        onInstall={(targetId) => void runInstall(targetId)}
      />

      <SkillSourceDialog
        open={sourceOpen}
        busy={sourceBusy}
        error={sourceError}
        onClose={() => {
          setSourceOpen(false);
          setSourceError(null);
        }}
        onDirty={() => setSourceError(null)}
        onAdd={(input) => void doAddSource(input)}
      />

      <SkillUninstallDialog
        open={uninstallOpen}
        count={allPaths.length}
        busy={removingAll}
        onClose={() => setUninstallOpen(false)}
        onConfirm={() => void doRemoveAll()}
      />
    </>
  );
}
