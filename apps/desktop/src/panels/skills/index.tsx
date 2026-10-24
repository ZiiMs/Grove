import type {
  CatalogEntry,
  SkillCatalog,
  SkillPreview,
  SkillTarget,
} from "@grove/contract";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useBridge } from "../../ipc/context";
import { PanelFrame } from "../PanelFrame";
import "./skills.css";

interface InstallState {
  kind: "success" | "error";
  message: string;
}

function errorMessage(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string") return message;
  }
  if (error instanceof Error) return error.message;
  return String(error);
}

/**
 * Browse public GitHub skill catalogs, preview a skill, and install it into the harness you use.
 * Everything goes through `useBridge()`: the panel never sees Tauri and stays testable under
 * `FakeBridge`. When the catalog is served from cache the panel says so and remains usable offline.
 */
export function SkillsPanel() {
  const bridge = useBridge();
  const [catalog, setCatalog] = useState<SkillCatalog | null>(null);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [targets, setTargets] = useState<SkillTarget[]>([]);
  const [targetId, setTargetId] = useState("");
  const [selected, setSelected] = useState<CatalogEntry | null>(null);
  const [preview, setPreview] = useState<SkillPreview | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [installState, setInstallState] = useState<InstallState | null>(null);
  const [installing, setInstalling] = useState(false);

  const loadCatalog = useCallback(
    async (refresh: boolean) => {
      setLoading(true);
      setCatalogError(null);
      try {
        setCatalog(await bridge.invoke("skill_catalog", { refresh }));
      } catch (error) {
        setCatalogError(errorMessage(error));
      } finally {
        setLoading(false);
      }
    },
    [bridge],
  );

  useEffect(() => {
    void loadCatalog(false);
  }, [loadCatalog]);

  useEffect(() => {
    let cancelled = false;
    bridge
      .invoke("skill_targets", {})
      .then((next) => {
        if (cancelled) return;
        setTargets(next);
        setTargetId((current) => current || next[0]?.id || "");
      })
      .catch(() => {
        // The catalog is still browsable without a destination; install stays disabled.
      });
    return () => {
      cancelled = true;
    };
  }, [bridge]);

  const selectSkill = useCallback(
    async (entry: CatalogEntry) => {
      setSelected(entry);
      setPreview(null);
      setPreviewError(null);
      setInstallState(null);
      try {
        setPreview(
          await bridge.invoke("skill_preview", {
            source: entry.source,
            path: entry.path,
          }),
        );
      } catch (error) {
        setPreviewError(errorMessage(error));
      }
    },
    [bridge],
  );

  const install = useCallback(async () => {
    if (!selected || !targetId) return;
    setInstalling(true);
    setInstallState(null);
    try {
      const result = await bridge.invoke("skill_install", {
        source: selected.source,
        path: selected.path,
        target: targetId,
      });
      setInstallState({
        kind: "success",
        message: `Installed ${result.skill} into ${result.path} (${
          result.overwritten ? "overwritten" : "new"
        }).`,
      });
    } catch (error) {
      setInstallState({
        kind: "error",
        message: `Install failed: ${errorMessage(error)}`,
      });
    } finally {
      setInstalling(false);
    }
  }, [bridge, selected, targetId]);

  const visibleEntries = useMemo(() => {
    const entries = catalog?.entries ?? [];
    const needle = query.trim().toLowerCase();
    if (!needle) return entries;
    return entries.filter((entry) =>
      `${entry.name} ${entry.source} ${entry.path}`.toLowerCase().includes(needle),
    );
  }, [catalog, query]);

  const canInstall = Boolean(selected && preview && targetId) && !installing;

  return (
    <PanelFrame
      title="Skills"
      subtitle="Browse public catalogs and install a skill into the harness you use."
      actions={
        <button
          type="button"
          className="input"
          onClick={() => void loadCatalog(true)}
          disabled={loading}
        >
          Refresh catalog
        </button>
      }
    >
      <div className="skills-panel">
        <div className="skills-browser">
          <input
            className="input skills-search"
            type="search"
            aria-label="Search skills"
            placeholder="Search by name, source or path"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />

          {catalog?.offline ? (
            <p className="skills-banner is-warning" data-testid="catalog-offline">
              Offline — showing the cached catalog.
            </p>
          ) : null}
          {catalog?.fromCache && !catalog.offline ? (
            <p className="skills-banner" data-testid="catalog-cached">
              Showing the cached catalog.
            </p>
          ) : null}

          {catalogError ? (
            <div className="skills-banner is-error" data-testid="catalog-error">
              <p>Catalog unavailable: {catalogError}</p>
              <button type="button" className="input" onClick={() => void loadCatalog(false)}>
                Retry
              </button>
            </div>
          ) : null}

          {loading && !catalog ? <p className="skills-muted">Loading skill catalog…</p> : null}

          {catalog && visibleEntries.length === 0 ? (
            <p className="skills-muted" data-testid="catalog-empty">
              No skills match “{query}”.
            </p>
          ) : null}

          <ul className="skills-list" aria-label="Skill catalog">
            {visibleEntries.map((entry) => (
              <li key={entry.id}>
                <button
                  type="button"
                  className={`skills-entry${
                    selected?.id === entry.id ? " is-selected" : ""
                  }`}
                  onClick={() => void selectSkill(entry)}
                >
                  <span className="skills-entry-name">{entry.name}</span>
                  <span className="skills-entry-source">
                    {entry.source} · {entry.path}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </div>

        <div className="skills-preview-pane">
          {selected ? (
            <section className="skills-preview" data-testid="skill-preview" aria-label="Skill preview">
              <header>
                <h2 className="skills-preview-title">{selected.name}</h2>
                <p className="skills-muted">
                  {selected.source} · {selected.path}
                </p>
              </header>

              {previewError ? (
                <p className="skills-banner is-error" data-testid="preview-error">
                  Preview unavailable: {previewError}
                </p>
              ) : null}
              {!preview && !previewError ? (
                <p className="skills-muted">Loading preview…</p>
              ) : null}

              {preview ? (
                <>
                  <dl className="skills-frontmatter" data-testid="skill-frontmatter">
                    {Object.entries(preview.frontmatter).map(([key, value]) => (
                      <div className="skills-frontmatter-row" key={key}>
                        <dt>{key}</dt>
                        <dd>{value}</dd>
                      </div>
                    ))}
                  </dl>
                  <pre className="skills-body" data-testid="skill-body">
                    {preview.body}
                  </pre>
                </>
              ) : null}

              <footer className="skills-install">
                <label className="skills-destination">
                  <span>Destination</span>
                  <select
                    className="input"
                    aria-label="Destination"
                    value={targetId}
                    onChange={(event) => setTargetId(event.target.value)}
                    disabled={targets.length === 0}
                  >
                    {targets.length === 0 ? <option value="">No destinations</option> : null}
                    {targets.map((target) => (
                      <option key={target.id} value={target.id}>
                        {target.label} — {target.path}
                        {target.exists ? "" : " (new)"}
                      </option>
                    ))}
                  </select>
                </label>
                <button
                  type="button"
                  className="input primary"
                  onClick={() => void install()}
                  disabled={!canInstall}
                >
                  Install skill
                </button>
                {installState ? (
                  <p
                    className={`skills-banner ${
                      installState.kind === "success" ? "is-success" : "is-error"
                    }`}
                    data-testid={
                      installState.kind === "success" ? "install-success" : "install-error"
                    }
                  >
                    {installState.message}
                  </p>
                ) : null}
              </footer>
            </section>
          ) : (
            <p className="skills-muted">Select a skill to preview it.</p>
          )}
        </div>
      </div>
    </PanelFrame>
  );
}
