/**
 * PROTOTYPE — throwaway. Git panel: branch table vs PR stack vs worktree cards.
 */

import { changedFiles, diff, gitRows } from "../fixtures";
import type { VariantDef } from "./types";

function dot(state: string) {
  return `pv-status-dot ${state === "live" ? "is-live" : "is-" + state}`;
}

/* ------------------------------------------------------------------ A — table */

function BranchTableLayout() {
  return (
    <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
      <div className="pv-row">
        <span className="pv-badge is-accent">4 worktrees</span>
        <span className="pv-faint pv-small">one row per worktree, not per branch</span>
        <span className="pv-rail-spacer" style={{ flex: 1 }} />
        <button type="button" className="pv-btn sz-sm">Fetch</button>
        <button type="button" className="pv-btn sz-sm is-primary">New worktree</button>
      </div>
      <div className="pv-card">
        <div className="pv-table cols-git">
          <div className="pv-table-head">
            <span />
            <span>branch</span>
            <span>sync</span>
            <span>pr</span>
          </div>
          {gitRows.map((row) => (
            <div key={row.branch} className="pv-table-row">
              <span className={dot(row.state)} />
              <span className="pv-branch">⑂ {row.branch}</span>
              <span className="pv-small pv-faint">{row.sync}</span>
              <span className="pv-small">{row.pr}</span>
            </div>
          ))}
        </div>
      </div>
      <div className="pv-card">
        <div className="pv-card-head">
          <b>Changes</b>
          <span className="pv-badge">4 files</span>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <span style={{ color: "var(--success)" }} className="pv-small">+62</span>
          <span style={{ color: "var(--danger)" }} className="pv-small">−3</span>
          <button type="button" className="pv-btn sz-sm is-primary">Commit &amp; push</button>
        </div>
        {changedFiles.map((file) => (
          <div key={file.path} className="pv-table-row" style={{ gridTemplateColumns: "1fr 52px 44px" }}>
            <span className="pv-mono">{file.path}</span>
            <span style={{ color: "var(--success)" }}>+{file.added}</span>
            <span style={{ color: "var(--danger)" }}>−{file.removed}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------- B — PR stack */

function StackLayout() {
  return (
    <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
      <div className="pv-row">
        <span className="pv-title" style={{ fontSize: "var(--font-lg)" }}>Stack</span>
        <span className="pv-badge">3 PRs</span>
        <span className="pv-rail-spacer" style={{ flex: 1 }} />
        <span className="pv-small pv-faint">each PR is a branch on top of the one below</span>
      </div>

      <div className="pv-stack">
        {gitRows.slice(0, 3).map((row, index) => (
          <div key={row.branch} className="pv-stack-row">
            <div className="pv-stack-spine">
              <span className={dot(row.state)} style={{ marginTop: 14 }} />
              {index < 2 ? <i /> : null}
            </div>
            <div className="pv-card" style={{ flex: 1 }}>
              <div className="pv-card-head">
                <span className="pv-badge is-accent">{row.pr}</span>
                <b className="pv-branch">{row.branch}</b>
                <span className="pv-rail-spacer" style={{ flex: 1 }} />
                <span className="pv-badge">{row.checks} checks</span>
                <button type="button" className="pv-btn sz-sm">Review</button>
                <button type="button" className="pv-btn sz-sm is-primary">Merge</button>
              </div>
              <div className="pv-card-body">
                <div className="pv-row">
                  <span className="pv-small pv-faint">worktree {row.worktree}</span>
                  <span className="pv-small pv-faint">·</span>
                  <span className="pv-small pv-faint">{row.sync}</span>
                </div>
                <div className="pv-diff">
                  {diff.rows.slice(0, 3).map((line) => (
                    <div key={`${line.ln}-${line.text}`} className={`pv-diff-row is-${line.kind}`}>
                      <span className="ln">{line.ln || ""}</span>
                      <span className="tx">
                        {line.kind === "add" ? "+" : line.kind === "del" ? "−" : " "}
                        {line.text}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------- C — worktrees */

function WorktreeLayout() {
  return (
    <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
      <div className="pv-row">
        <span className="pv-title" style={{ fontSize: "var(--font-lg)" }}>Worktrees</span>
        <span className="pv-rail-spacer" style={{ flex: 1 }} />
        <button type="button" className="pv-btn sz-sm is-primary">New from issue…</button>
      </div>
      <div className="pv-worktrees">
        {gitRows.map((row) => (
          <div key={row.branch} className="pv-card">
            <div className="pv-card-head">
              <span className={dot(row.state)} />
              <b className="pv-branch">{row.branch}</b>
              <span className="pv-rail-spacer" style={{ flex: 1 }} />
              <span className="pv-faint">⋯</span>
            </div>
            <div className="pv-card-body">
              <div className="pv-row">
                <span className="pv-badge">{row.sync}</span>
                <span className="pv-badge">{row.pr}</span>
                <span className="pv-badge">{row.checks}</span>
              </div>
              <div className="pv-small pv-faint pv-mono">{row.worktree}</div>
              <div className="pv-row">
                <button type="button" className="pv-btn sz-sm">Open</button>
                <button type="button" className="pv-btn sz-sm is-ghost">Diff</button>
                <span className="pv-rail-spacer" style={{ flex: 1 }} />
                <button type="button" className="pv-btn sz-sm is-primary">PR</button>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export const gitVariants: VariantDef[] = [
  { key: "A", name: "Branch table", bet: "Rows for worktrees plus a changes list — everything on one screen, nothing hidden.", Component: BranchTableLayout },
  { key: "B", name: "PR stack", bet: "Pull requests as a stack with checks and merge inline — landing work is the only job.", Component: StackLayout },
  { key: "C", name: "Worktrees", chosen: true, bet: "A card per worktree — matches the isolation model, and each card is a place to start work.", Component: WorktreeLayout },
];
