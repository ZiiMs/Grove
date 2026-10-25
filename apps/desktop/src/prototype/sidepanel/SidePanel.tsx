/**
 * PROTOTYPE — throwaway. The right sidepanel: four contextual tabs (Plan · Specs · Changes · Git),
 * collapsible to a strip that keeps its glyphs and counts. Three presentations of the contents.
 */

import { useState } from "react";
import { changedFiles, gitRows, plan, specs } from "../fixtures";

export type SideTab = "plan" | "specs" | "changes" | "git";
export type SideVariant = "A" | "B" | "C";

export const sideTabs: { id: SideTab; label: string; glyph: string }[] = [
  { id: "plan", label: "Plan", glyph: "☰" },
  { id: "specs", label: "Specs", glyph: "◈" },
  { id: "changes", label: "Changes", glyph: "±" },
  { id: "git", label: "Git", glyph: "⑂" },
];

const pinnedSpecs = specs.filter((spec) => ["arch", "a03", "a05"].includes(spec.id));

function counts(tab: SideTab): number {
  if (tab === "plan") return plan.length;
  if (tab === "specs") return pinnedSpecs.length;
  if (tab === "changes") return changedFiles.length;
  return gitRows.length;
}

function Pinned({ children }: { children: React.ReactNode }) {
  return <div className="pv-pinned">{children}</div>;
}

function PlanTab({ variant }: { variant: SideVariant }) {
  const done = plan.filter((item) => item.state === "done").length;

  if (variant === "B") {
    return (
      <div className="pv-side-stack">
        {plan.map((item) => (
          <div key={item.id} className={item.state === "now" ? "pv-ticket is-now" : "pv-ticket"}>
            <span>{item.text}</span>
            <span className="pv-badge">{item.state}</span>
          </div>
        ))}
      </div>
    );
  }

  return (
    <div className="pv-side-stack">
      {variant === "C" ? (
        <Pinned>
          <div className="pv-row">
            <span className="pv-status-dot is-live" />
            <b className="pv-small">Claude Code · working</b>
          </div>
          <div className="pv-small pv-faint">1 approval waiting · {done} of {plan.length} done</div>
          <div className="pv-progress"><i style={{ width: `${(done / plan.length) * 100}%` }} /></div>
        </Pinned>
      ) : (
        <div className="pv-progress"><i style={{ width: `${(done / plan.length) * 100}%` }} /></div>
      )}
      <div className="pv-side-list">
        {plan.map((item) => (
          <div key={item.id} className="pv-side-row">
            <span
              className={
                item.state === "done"
                  ? "pv-gutter is-done"
                  : item.state === "now"
                    ? "pv-gutter is-now"
                    : item.state === "blocked"
                      ? "pv-gutter is-blocked"
                      : "pv-gutter"
              }
              style={{ width: 18, height: 18, fontSize: 9 }}
            >
              {item.state === "done" ? "✓" : item.state === "blocked" ? "!" : ""}
            </span>
            <span className={item.state === "done" ? "pv-side-text is-done" : "pv-side-text"}>{item.text}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function SpecsTab({ variant }: { variant: SideVariant }) {
  if (variant === "B") {
    return (
      <div className="pv-side-stack">
        {pinnedSpecs.map((spec) => (
          <div key={spec.id} className="pv-ticket">
            <b className="pv-small pv-mono">{spec.name}</b>
            <div className="pv-row">
              <span className="pv-badge">{spec.type}</span>
              <span className="pv-faint pv-small">{spec.edge || "root"}</span>
            </div>
          </div>
        ))}
      </div>
    );
  }
  return (
    <div className="pv-side-stack">
      {variant === "C" ? (
        <Pinned>
          <b className="pv-small">05-session-persistence</b>
          <span className="pv-small pv-faint">pinned to this session · 3 edges</span>
        </Pinned>
      ) : null}
      <div className="pv-side-list">
        {pinnedSpecs.map((spec) => (
          <div key={spec.id} className="pv-side-row">
            <span className="pv-faint">◇</span>
            <span className="pv-side-text pv-mono">{spec.name}</span>
            <span className="pv-badge">{spec.type}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function ChangesTab({ variant }: { variant: SideVariant }) {
  if (variant === "B") {
    return (
      <div className="pv-side-stack">
        {changedFiles.map((file) => (
          <div key={file.path} className="pv-ticket">
            <b className="pv-small pv-mono">{file.path}</b>
            <div className="pv-row">
              <span style={{ color: "var(--success)" }} className="pv-small">+{file.added}</span>
              <span style={{ color: "var(--danger)" }} className="pv-small">−{file.removed}</span>
              <span className="pv-rail-spacer" style={{ flex: 1 }} />
              <span className="pv-badge">{file.state}</span>
            </div>
          </div>
        ))}
      </div>
    );
  }
  return (
    <div className="pv-side-stack">
      {variant === "C" ? (
        <Pinned>
          <div className="pv-row">
            <span className="pv-status-dot is-warn" />
            <b className="pv-small">1 approval waiting</b>
          </div>
          <div className="pv-row">
            <button type="button" className="pv-btn sz-sm is-primary">Allow once</button>
            <button type="button" className="pv-btn sz-sm is-ghost">Deny</button>
          </div>
        </Pinned>
      ) : null}
      <div className="pv-side-list">
        {changedFiles.map((file) => (
          <div key={file.path} className="pv-side-row">
            <span className="pv-side-text pv-mono">{file.path}</span>
            <span style={{ color: "var(--success)" }} className="pv-small">+{file.added}</span>
            <span style={{ color: "var(--danger)" }} className="pv-small">−{file.removed}</span>
          </div>
        ))}
      </div>
      <button type="button" className="pv-btn sz-sm is-primary">Open PR</button>
    </div>
  );
}

function GitTab({ variant }: { variant: SideVariant }) {
  if (variant === "B") {
    return (
      <div className="pv-side-stack">
        {gitRows.map((row) => (
          <div key={row.branch} className="pv-ticket">
            <b className="pv-small pv-mono">⑂ {row.branch}</b>
            <div className="pv-row">
              <span className="pv-badge">{row.sync}</span>
              <span className="pv-faint pv-small">{row.pr}</span>
            </div>
          </div>
        ))}
      </div>
    );
  }
  return (
    <div className="pv-side-stack">
      {variant === "C" ? (
        <Pinned>
          <div className="pv-row">
            <span className="pv-status-dot is-live" />
            <b className="pv-small pv-mono">grove-project-setup</b>
          </div>
          <span className="pv-small pv-faint">ahead 3 · 4 worktrees · #12 draft</span>
        </Pinned>
      ) : null}
      <div className="pv-side-list">
        {gitRows.map((row) => (
          <div key={row.branch} className="pv-side-row">
            <span className="pv-faint">⑂</span>
            <span className="pv-side-text pv-mono">{row.branch}</span>
            <span className="pv-faint pv-small">{row.sync}</span>
          </div>
        ))}
      </div>
      <button type="button" className="pv-btn sz-sm">New worktree</button>
    </div>
  );
}

interface Props {
  variant?: SideVariant;
  collapsed: boolean;
  onToggle: () => void;
  initialTab?: SideTab;
  /** Lets the shell keep the prototype's explored section in sync with the visible tab. */
  onTab?: (tab: SideTab) => void;
}

export function SidePanel({ variant = "A", collapsed, onToggle, initialTab = "plan", onTab }: Props) {
  const [tab, setTab] = useState<SideTab>(initialTab);

  const select = (next: SideTab) => {
    setTab(next);
    onTab?.(next);
  };

  if (collapsed) {
    return (
      <aside className="pv-sidepanel is-collapsed" aria-label="Side panel (collapsed)">
        <button type="button" className="pv-side-expand" onClick={onToggle} aria-label="Expand side panel">
          ‹
        </button>
        {sideTabs.map((entry) => (
          <button
            key={entry.id}
            type="button"
            className="pv-side-rail-btn"
            title={entry.label}
            aria-label={`Open ${entry.label}`}
            onClick={() => {
              setTab(entry.id);
              onToggle();
            }}
          >
            <span>{entry.glyph}</span>
            <span className="pv-side-rail-count">{counts(entry.id)}</span>
          </button>
        ))}
      </aside>
    );
  }

  return (
    <aside className="pv-sidepanel" aria-label="Side panel">
      <div className="pv-side-tabs" role="tablist">
        {sideTabs.map((entry) => (
          <button
            key={entry.id}
            type="button"
            role="tab"
            aria-selected={entry.id === tab}
            className={entry.id === tab ? "pv-side-tab is-active" : "pv-side-tab"}
            onClick={() => select(entry.id)}
          >
            <span className="pv-side-glyph">{entry.glyph}</span>
            {entry.label}
            <span className="pv-side-count">{counts(entry.id)}</span>
          </button>
        ))}
        <button type="button" className="pv-side-collapse" onClick={onToggle} aria-label="Collapse side panel">
          ›
        </button>
      </div>
      <div className="pv-side-body">
        {tab === "plan" ? <PlanTab variant={variant} /> : null}
        {tab === "specs" ? <SpecsTab variant={variant} /> : null}
        {tab === "changes" ? <ChangesTab variant={variant} /> : null}
        {tab === "git" ? <GitTab variant={variant} /> : null}
      </div>
    </aside>
  );
}

/** Section helper: three sidepanels side by side, each with its own tab and collapse state. */
export function SidePanelDemo({ variant }: { variant: SideVariant }) {
  const [collapsed, setCollapsed] = useState(false);
  return (
    <div style={{ width: 300, height: 420, display: "flex", border: "1px solid var(--border)" }}>
      <SidePanel variant={variant} collapsed={collapsed} onToggle={() => setCollapsed(!collapsed)} />
    </div>
  );
}
