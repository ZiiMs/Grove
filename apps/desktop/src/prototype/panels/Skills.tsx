/**
 * PROTOTYPE — throwaway. Skills, third attempt: catalog-first. You see the catalogs, open one, and
 * pick skills from inside it. Three structural takes on that.
 */

import { catalogSkills, catalogs, installTargets } from "../fixtures";
import type { VariantDef } from "./types";

const openCatalog = catalogs[0];
const openSkills = catalogSkills.filter((skill) => skill.catalog === openCatalog.id);

function Targets({ installed }: { installed: string[] }) {
  if (installed.length === 0) return <span className="pv-faint pv-small">—</span>;
  return (
    <span className="pv-row" style={{ gap: 3 }}>
      {installed.map((target) => (
        <span key={target} className="pv-badge is-success">{target}</span>
      ))}
    </span>
  );
}

/* --------------------------------------------------- A — sources → catalog */

function SourcesLayout() {
  return (
    <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
      <div className="pv-row">
        <span className="pv-title" style={{ fontSize: "var(--font-lg)" }}>Catalogs</span>
        <span className="pv-badge">{catalogs.length} sources</span>
        <span className="pv-faint pv-small">pick a catalog, then pick skills inside it</span>
        <span className="pv-rail-spacer" style={{ flex: 1 }} />
        <button type="button" className="pv-btn sz-sm is-ghost">＋ Add source</button>
      </div>

      <div className="pv-skill-grid" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(240px, 1fr))" }}>
        {catalogs.map((catalog) => (
          <div
            key={catalog.id}
            className="pv-card"
            style={catalog.id === openCatalog.id ? { borderColor: "var(--accent)" } : undefined}
          >
            <div className="pv-card-head">
              <b className="pv-mono pv-small">{catalog.owner}/{catalog.name}</b>
              <span className="pv-rail-spacer" style={{ flex: 1 }} />
              <span className="pv-badge">{catalog.count}</span>
            </div>
            <div className="pv-card-body">
              <span className="pv-small pv-muted">{catalog.blurb}</span>
              <div className="pv-row">
                <span className="pv-badge is-success">{catalog.installed} installed</span>
                <span className="pv-faint pv-small">{catalog.updated}</span>
                <span className="pv-rail-spacer" style={{ flex: 1 }} />
                <button type="button" className="pv-btn sz-sm">Open</button>
              </div>
            </div>
          </div>
        ))}
      </div>

      <div className="pv-card" style={{ borderColor: "var(--accent-line)" }}>
        <div className="pv-card-head">
          <span className="pv-faint pv-small">catalogs /</span>
          <b className="pv-mono">{openCatalog.owner}/{openCatalog.name}</b>
          <span className="pv-badge">{openCatalog.count} skills</span>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <select className="pv-select" style={{ width: 150, height: 24 }} defaultValue="claude">
            {installTargets.map((target) => (
              <option key={target} value={target}>install into {target}</option>
            ))}
          </select>
        </div>
        <div className="pv-side-list" style={{ padding: 6 }}>
          {openSkills.map((skill) => (
            <div key={skill.name} className="pv-side-row" style={{ height: 40 }}>
              <span style={{ color: "var(--accent)" }}>✦</span>
              <span className="pv-side-text">
                <b className="pv-small">{skill.name}</b>
                <span className="pv-faint pv-small"> — {skill.blurb}</span>
              </span>
              <Targets installed={skill.installed} />
              <span className="pv-faint pv-small">{skill.updated}</span>
              <button type="button" className="pv-btn sz-sm is-primary">Install</button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

/* ---------------------------------------------- B — catalog rail + skills */

function RailLayout() {
  return (
    <div className="pv-master is-narrow">
      <div className="pv-pane">
        <div className="pv-pane-head">
          <b>Catalogs</b>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <span className="pv-faint">＋</span>
        </div>
        <div className="pv-scroll pv-tree" style={{ padding: 6 }}>
          {catalogs.map((catalog) => (
            <div key={catalog.id} className={catalog.id === openCatalog.id ? "pv-tree-row is-active" : "pv-tree-row"}>
              <span className="pv-faint">▸</span>
              <span className="pv-tree-name pv-mono pv-small">{catalog.owner}/{catalog.name}</span>
              <span className="pv-nav-count">{catalog.count}</span>
            </div>
          ))}
        </div>
      </div>
      <div className="pv-pane">
        <div className="pv-pane-head">
          <b className="pv-mono">{openCatalog.owner}/{openCatalog.name}</b>
          <span className="pv-badge">{openCatalog.installed} installed</span>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <select className="pv-select" style={{ width: 140, height: 24 }} defaultValue="claude">
            {installTargets.map((target) => (
              <option key={target} value={target}>{target}</option>
            ))}
          </select>
        </div>
        <div className="pv-scroll">
          <div className="pv-table-head" style={{ gridTemplateColumns: "1fr 90px 80px 70px", padding: "0 10px" }}>
            <span>skill</span>
            <span>installed in</span>
            <span>updated</span>
            <span />
          </div>
          {openSkills.map((skill) => (
            <div
              key={skill.name}
              className="pv-table-row"
              style={{ gridTemplateColumns: "1fr 90px 80px 70px", height: "auto", padding: "8px 10px", gap: 8 }}
            >
              <div className="pv-col" style={{ gap: 1 }}>
                <b className="pv-small">{skill.name}</b>
                <span className="pv-faint pv-small">{skill.blurb}</span>
              </div>
              <Targets installed={skill.installed} />
              <span className="pv-faint pv-small">{skill.updated}</span>
              <button type="button" className="pv-btn sz-sm">Install</button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

/* ---------------------------------------------------- C — faceted one list */

function FacetedLayout() {
  return (
    <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
      <div className="pv-row">
        <div className="pv-input-wrap" style={{ flex: 1, maxWidth: 340 }}>
          <span className="pv-faint">⌕</span>
          <input className="pv-input" placeholder="Search 401 skills in 6 catalogs…" />
        </div>
        <span className="pv-rail-spacer" style={{ flex: 1 }} />
        <button type="button" className="pv-btn sz-sm is-ghost">＋ Add source</button>
      </div>

      <div className="pv-row" style={{ gap: 4 }}>
        <span className="pv-chip" style={{ borderColor: "var(--accent)", color: "var(--accent)" }}>
          all 401
        </span>
        {catalogs.map((catalog) => (
          <span key={catalog.id} className="pv-chip">
            {catalog.owner}/{catalog.name}
            <span className="pv-faint">{catalog.count}</span>
          </span>
        ))}
      </div>

      <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
        {["anthropics", "mattpocock", "grove"].map((catalogId) => {
          const catalog = catalogs.find((entry) => entry.id === catalogId);
          const rows = catalogSkills.filter((skill) => skill.catalog === catalogId);
          if (!catalog) return null;
          return (
            <div key={catalogId} className="pv-card">
              <div className="pv-card-head">
                <b className="pv-mono pv-small">{catalog.owner}/{catalog.name}</b>
                <span className="pv-badge">{catalog.count}</span>
                <span className="pv-faint pv-small">{catalog.blurb}</span>
                <span className="pv-rail-spacer" style={{ flex: 1 }} />
                <button type="button" className="pv-btn sz-sm is-ghost">View all {catalog.count}</button>
              </div>
              <div className="pv-side-list" style={{ padding: 6 }}>
                {rows.map((skill) => (
                  <div key={skill.name} className="pv-side-row" style={{ height: 36 }}>
                    <span style={{ color: "var(--accent)" }}>✦</span>
                    <span className="pv-side-text">
                      <b className="pv-small">{skill.name}</b>
                      <span className="pv-faint pv-small"> — {skill.blurb}</span>
                    </span>
                    <Targets installed={skill.installed} />
                    <select className="pv-select" style={{ width: 92, height: 22 }} defaultValue="claude">
                      {installTargets.map((target) => (
                        <option key={target} value={target}>{target}</option>
                      ))}
                    </select>
                    <button type="button" className="pv-btn sz-sm">Install</button>
                  </div>
                ))}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

export const skillsVariants: VariantDef[] = [
  { key: "A", name: "Sources, then open", bet: "Catalogs are the page; opening one shows its skills beneath — a drill-down, so the catalog is a place.", Component: SourcesLayout },
  { key: "B", name: "Catalog rail", chosen: true, bet: "Catalogs as a permanent left rail, skills as a table — the fastest way to sweep one catalog end to end.", Component: RailLayout },
  { key: "C", name: "One faceted list", bet: "Every skill in one searchable list, grouped by catalog, filtered by source chips — no page to open first.", Component: FacetedLayout },
];
