/**
 * PROTOTYPE — throwaway. Changes: the working-tree view that shares the right panel with Plan, Specs
 * and Git. Diff stat first, then commit/PR actions.
 */

import { changedFiles, diff } from "../fixtures";

export function ChangesView() {
  return (
    <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
      <div className="pv-row">
        <span className="pv-title" style={{ fontSize: "var(--font-lg)" }}>Changes</span>
        <span className="pv-badge">{changedFiles.length} files</span>
        <span className="pv-small" style={{ color: "var(--success)" }}>
          +{changedFiles.reduce((sum, file) => sum + file.added, 0)}
        </span>
        <span className="pv-small" style={{ color: "var(--danger)" }}>
          −{changedFiles.reduce((sum, file) => sum + file.removed, 0)}
        </span>
        <span className="pv-rail-spacer" style={{ flex: 1 }} />
        <button type="button" className="pv-btn sz-sm">Discard</button>
        <button type="button" className="pv-btn sz-sm is-primary">Commit &amp; push</button>
      </div>

      <div className="pv-card">
        <div className="pv-card-head">
          <b>Unstaged</b>
          <span className="pv-faint">working tree · grove-project-setup</span>
        </div>
        {changedFiles.map((file) => (
          <div key={file.path} className="pv-table-row" style={{ gridTemplateColumns: "1fr 56px 48px 46px" }}>
            <span className="pv-mono pv-small">{file.path}</span>
            <span className="pv-faint pv-small">{file.state}</span>
            <span style={{ color: "var(--success)" }}>+{file.added}</span>
            <span style={{ color: "var(--danger)" }}>−{file.removed}</span>
          </div>
        ))}
      </div>

      <div className="pv-diff">
        <div className="pv-diff-head">
          <span className="pv-badge">diff</span>
          <b className="pv-mono">{diff.path}</b>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <span style={{ color: "var(--success)" }}>+{diff.added}</span>
          <span style={{ color: "var(--danger)" }}>−{diff.removed}</span>
        </div>
        {diff.rows.map((row) => (
          <div key={`${row.kind}-${row.ln}-${row.text}`} className={`pv-diff-row is-${row.kind}`}>
            <span className="ln">{row.ln || ""}</span>
            <span className="tx">
              {row.kind === "add" ? "+" : row.kind === "del" ? "−" : " "}
              {row.text}
            </span>
          </div>
        ))}
      </div>

      <div className="pv-field">
        <span className="pv-label">Commit message</span>
        <input className="pv-input" defaultValue="feat(adapters): add the ACP adapter behind HarnessAdapter" />
      </div>
    </div>
  );
}
