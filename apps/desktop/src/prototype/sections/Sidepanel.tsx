/**
 * PROTOTYPE — throwaway. Sidepanel section: the right column's anatomy and three tab presentations.
 */

import { SidePanel, SidePanelDemo } from "../sidepanel/SidePanel";

const presentations = [
  {
    variant: "A" as const,
    name: "Rows",
    bet: "One line per item, counts in the tab labels. Densest, and the default in the live shell.",
  },
  {
    variant: "B" as const,
    name: "Cards",
    bet: "A compact card per item — room for source, targets and state, at roughly half the items per screen.",
  },
  {
    variant: "C" as const,
    name: "Pinned first",
    bet: "A pinned block answers \"what does this need from me\" before the list — approval, progress, active spec.",
  },
];

export function SidepanelSection() {
  return (
    <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
      <div className="pv-row">
        <span className="pv-badge is-accent">right panel</span>
        <span className="pv-small pv-faint">
          fixed frame · four tabs · collapses to a 40px strip that keeps its counts
        </span>
      </div>

      <div className="pv-row" style={{ alignItems: "stretch", gap: "var(--pad-x)", flexWrap: "wrap" }}>
        {presentations.map((entry) => (
          <div key={entry.variant} className="pv-col" style={{ gap: 6, width: 300 }}>
            <div className="pv-row">
              <span className="pv-badge is-accent">{entry.variant}</span>
              <b className="pv-small">{entry.name}</b>
            </div>
            <SidePanelDemo variant={entry.variant} />
            <span className="pv-small pv-faint">{entry.bet}</span>
          </div>
        ))}
      </div>

      <div className="pv-grid-2">
        <div className="pv-card">
          <div className="pv-card-head">
            <b>Collapsed strip</b>
            <span className="pv-faint">40px, one glyph per tab, counts survive</span>
          </div>
          <div className="pv-card-body">
            <div style={{ width: 40, height: 240, border: "1px solid var(--border)", display: "flex" }}>
              <SidePanelCollapsed />
            </div>
          </div>
        </div>

        <div className="pv-card">
          <div className="pv-card-head">
            <b>Anatomy</b>
            <span className="pv-faint">what each tab owns</span>
          </div>
          <div className="pv-card-body">
            <div className="pv-side-list">
              <div className="pv-side-row">
                <span className="pv-side-glyph">☰</span>
                <span className="pv-side-text">Plan — the session&apos;s steps, progress and the one marked now</span>
              </div>
              <div className="pv-side-row">
                <span className="pv-side-glyph">◈</span>
                <span className="pv-side-text">Specs — only the specs pinned to this session, with edge labels</span>
              </div>
              <div className="pv-side-row">
                <span className="pv-side-glyph">✦</span>
                <span className="pv-side-text">Skills — attached here, plus which harness each is installed in</span>
              </div>
              <div className="pv-side-row">
                <span className="pv-side-glyph">⑂</span>
                <span className="pv-side-text">Changes — files touched with diff stat, approvals, open-PR action</span>
              </div>
            </div>
            <span className="pv-small pv-faint">
              Nothing in the panel duplicates a left-nav destination: the panel is session context, the
              rail is the product&apos;s seven surfaces.
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}

/** The collapsed strip on its own, for the specimen card. */
function SidePanelCollapsed() {
  return (
    <SidePanel
      variant="A"
      collapsed
      onToggle={() => undefined}
      initialTab="plan"
    />
  );
}
