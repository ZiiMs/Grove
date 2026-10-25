import type { CatalogEntry } from "@grove/contract";
import { useEffect, useMemo, useRef, useState } from "react";
import { useBridge } from "../ipc/context";
import { useSkillsStore } from "../state/skillsStore";
import { useUiStore } from "../state/uiStore";
import { Icon } from "../workspace/Icon";

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

export function SkillsPane({ projectRoot }: { projectRoot?: string }) {
  const bridge = useBridge();
  const catalog = useSkillsStore((state) => state.catalog);
  const installs = useSkillsStore((state) => state.installs);
  const targets = useSkillsStore((state) => state.targets);
  const preview = useSkillsStore((state) => state.preview);
  const selectedId = useSkillsStore((state) => state.selectedId);
  const query = useSkillsStore((state) => state.query);
  const loading = useSkillsStore((state) => state.loading);
  const previewLoading = useSkillsStore((state) => state.previewLoading);
  const installing = useSkillsStore((state) => state.installing);
  const error = useSkillsStore((state) => state.error);
  const load = useSkillsStore((state) => state.load);
  const select = useSkillsStore((state) => state.select);
  const setQuery = useSkillsStore((state) => state.setQuery);
  const install = useSkillsStore((state) => state.install);
  const remove = useSkillsStore((state) => state.remove);
  const notify = useUiStore((state) => state.notify);

  const loadedRoot = useRef<string | undefined>(undefined);
  const [catalogId, setCatalogId] = useState<string | null>(null);

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
    return [...map.entries()].sort(([left], [right]) => left.localeCompare(right));
  }, [allEntries]);

  useEffect(() => {
    if (catalogId && sources.some(([source]) => source === catalogId)) return;
    setCatalogId(sources[0]?.[0] ?? null);
  }, [catalogId, sources]);

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

  const selected =
    visible.find((entry) => entry.id === selectedId) ??
    allEntries.find((entry) => entry.id === selectedId) ??
    null;
  const frontmatter = preview?.frontmatter ?? {};
  const installed = selected
    ? (installs.find(
        (record) => record.skill === selected.name && record.source === selected.source,
      ) ?? installs.find((record) => record.skill === selected.name))
    : undefined;
  const installedCount = new Set(installs.map((record) => `${record.source}::${record.skill}`)).size;
  const agentsTarget = targets.find((target) => target.id === "agents");
  const description =
    frontmatter.description ?? (preview ? firstParagraph(preview.body) : selected?.path ?? "");
  const adds = preview ? bodyBullets(preview.body) : [];
  const tags = (frontmatter.tags ?? "")
    .split(",")
    .map((tag) => tag.trim())
    .filter(Boolean);
  const currentName = sources.find(([source]) => source === catalogId)?.[0] ?? "All skills";

  const doInstall = async () => {
    if (!selected) return;
    const ok = await install(bridge, selected, projectRoot);
    if (ok) notify(`${selected.name} installed`);
    else notify(useSkillsStore.getState().error ?? "Install failed", "error");
  };

  const doRemove = async () => {
    if (!installed) return;
    const ok = await remove(bridge, installed);
    if (ok) notify(`${installed.skill} removed`);
    else notify(useSkillsStore.getState().error ?? "Remove failed", "error");
  };

  return (
    <>
      <div className="sk-head">
        <div className="sk-head-text">
          <h3>Skills</h3>
          <p>
            Pick a catalog, read what a skill changes, then install it. Installed skills are available
            to the agent through the project's <span className="mono">.agents/skills</span> folder.
          </p>
        </div>
        <div className="sk-head-tools">
          <div className="search">
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
          <span className="count-pill">{installedCount} installed</span>
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
        </div>

        <div className="sk-list-col">
          <div className="sk-list-head">
            <span className="sk-list-title">{searching ? "Search results" : currentName}</span>
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
                const record =
                  installs.find(
                    (item) => item.skill === entry.name && item.source === entry.source,
                  ) ?? installs.find((item) => item.skill === entry.name);
                return (
                  <button
                    className="sk-row"
                    type="button"
                    key={entry.id}
                    aria-current={entry.id === selectedId}
                    onClick={() => select(bridge, entry)}
                  >
                    <span className="sk-row-main">
                      <span className="sk-row-name">
                        <span className="n">{entry.name}</span>
                        {searching ? <span className="v">· {entry.source}</span> : null}
                      </span>
                      <span className="sk-row-desc">{entry.source}</span>
                    </span>
                    {installing === entry.id ? (
                      <span className="sk-state wait">installing…</span>
                    ) : record ? (
                      <span className="sk-state ok">
                        <Icon name="checkCircle" size="xs" />
                        installed
                      </span>
                    ) : null}
                  </button>
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
              <code className="sk-path">
                {agentsTarget?.path ?? ".agents/skills"}/{selected.name}/SKILL.md
              </code>
              <div className="sk-actions">
                {installed ? (
                  <>
                    <button
                      className="act-btn danger"
                      type="button"
                      aria-label={`Remove ${selected.name}`}
                      title="Remove skill"
                      onClick={() => void doRemove()}
                    >
                      <Icon name="trash" size="sm" />
                    </button>
                    <span className="sk-hint">Installed in {installed.target}</span>
                  </>
                ) : (
                  <button
                    className="act-btn primary"
                    type="button"
                    aria-label={`Install ${selected.name}`}
                    title="Install skill"
                    disabled={installing === selected.id}
                    onClick={() => void doInstall()}
                  >
                    <Icon name="download" size="sm" />
                  </button>
                )}
              </div>
              <p className="sk-hint">
                {installed
                  ? "Installed for this project. Remove deletes the copied folder."
                  : "Installing copies the skill into .agents/skills and makes it available to the agent."}
              </p>
            </>
          )}
          {error ? <p className="field-err">{error}</p> : null}
          {catalog?.offline ? (
            <p className="sk-hint">Offline — showing the cached catalog from {catalog.fetchedAt ?? "an earlier run"}.</p>
          ) : null}
        </div>
      </div>
    </>
  );
}
