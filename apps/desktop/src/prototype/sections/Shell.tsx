/**
 * PROTOTYPE — throwaway. Shell & chrome specimens: rail, toolbar, tabs, command bar, split panes.
 */

const railItems = [
  { glyph: "▶", label: "Sessions", active: true },
  { glyph: "✦", label: "Skills" },
  { glyph: "◈", label: "Specs" },
  { glyph: "☰", label: "Plan" },
  { glyph: "⚙", label: "MCP" },
  { glyph: "⑂", label: "Git" },
  { glyph: "⚑", label: "Settings" },
];

export function ShellSection() {
  return (
    <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
      <div className="pv-block">
        <div className="pv-block-head">
          <h2>Navigation rail</h2>
          <span>full labels vs icon-only — the icon rail buys ~150px of transcript width</span>
        </div>
        <div className="pv-grid-2">
          <div className="pv-card">
            <div className="pv-card-head">
              <b>Full rail</b> · 232px
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "232px 1fr", height: 260 }}>
              <div className="pv-rail" style={{ padding: 8 }}>
                <div className="pv-brand">
                  <span className="pv-brand-mark">◆</span>
                  <span className="pv-brand-word">Grove</span>
                  <span className="pv-brand-tag">proto</span>
                </div>
                {railItems.map((item) => (
                  <button
                    key={item.label}
                    type="button"
                    className={item.active ? "pv-rail-item is-active" : "pv-rail-item"}
                  >
                    <span className="pv-rail-glyph">{item.glyph}</span>
                    <span>{item.label}</span>
                  </button>
                ))}
                <div className="pv-rail-spacer" />
                <div className="pv-rail-foot">
                  <span>v0.1.0 · windows</span>
                </div>
              </div>
              <div style={{ background: "var(--bg-sunken)", padding: 10 }}>
                <div className="pv-faint pv-small">panel area — 232px rail</div>
              </div>
            </div>
          </div>

          <div className="pv-card">
            <div className="pv-card-head">
              <b>Icon rail</b> · 52px, labels on hover
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "52px 1fr", height: 260 }}>
              <div className="pv-rail" style={{ alignItems: "center", padding: "8px 4px" }}>
                <div className="pv-brand" style={{ padding: "4px 0 8px" }}>
                  <span className="pv-brand-mark">◆</span>
                </div>
                {railItems.map((item) => (
                  <button
                    key={item.label}
                    type="button"
                    className={item.active ? "pv-rail-item is-active" : "pv-rail-item"}
                    style={{ justifyContent: "center", width: 36, padding: 0 }}
                    title={item.label}
                  >
                    <span className="pv-rail-glyph">{item.glyph}</span>
                  </button>
                ))}
                <div className="pv-rail-spacer" />
                <div className="pv-rail-glyph" style={{ paddingBottom: 6 }}>
                  ⚑
                </div>
              </div>
              <div style={{ background: "var(--bg-sunken)", padding: 10 }}>
                <div className="pv-faint pv-small">panel area — 52px rail</div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div className="pv-block">
        <div className="pv-block-head">
          <h2>Toolbar & tabs</h2>
          <span>breadcrumb + panel tabs + actions, and the ⌘K command bar</span>
        </div>
        <div className="pv-card">
          <div className="pv-toolbar" style={{ borderBottom: "1px solid var(--border)" }}>
            <div className="pv-crumbs">
              <span>grove</span>
              <span>/</span>
              <b>sessions</b>
              <span>·</span>
              <span className="pv-mono">workspace-3</span>
            </div>
            <div className="pv-toolbar-actions">
              <span className="pv-badge is-accent">2 agents</span>
              <button type="button" className="pv-btn sz-sm is-ghost">
                ⟳
              </button>
              <button type="button" className="pv-btn sz-sm is-primary">
                New session
              </button>
            </div>
          </div>
          <div className="pv-row" style={{ padding: "10px var(--pad-x)", gap: 12 }}>
            <div className="pv-segmented">
              <button type="button" className="pv-seg is-active">
                Transcript
              </button>
              <button type="button" className="pv-seg">
                Diff
              </button>
              <button type="button" className="pv-seg">
                Files
              </button>
              <button type="button" className="pv-seg">
                Terminal
              </button>
            </div>
            <div className="pv-input-wrap" style={{ maxWidth: 340, flex: 1 }}>
              <span className="pv-faint">⌕</span>
              <input className="pv-input" placeholder="Search sessions, specs, skills…" />
              <span className="pv-kbd">⌘K</span>
            </div>
            <span className="pv-chip">
              <span className="pv-status-dot is-live" /> claude-code
            </span>
            <span className="pv-chip">
              <span className="pv-status-dot is-ok" /> codex
            </span>
          </div>
        </div>
      </div>

      <div className="pv-block">
        <div className="pv-block-head">
          <h2>Split panes</h2>
          <span>two agent sessions side by side; the seam is the interesting part at 1px</span>
        </div>
        <div className="pv-card">
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1px 1fr", height: 210 }}>
            <div style={{ padding: 12, display: "flex", flexDirection: "column", gap: 8 }}>
              <div className="pv-row">
                <span className="pv-avatar">CC</span>
                <b className="pv-small">Claude Code</b>
                <span className="pv-badge is-success">running</span>
                <span className="pv-rail-spacer" />
                <span className="pv-faint pv-small">4m 12s</span>
              </div>
              <div className="pv-thinking">Reading spec 03 — adapter interface…</div>
              <div className="pv-small pv-muted">
                Adding the ACP adapter behind <span className="pv-code">HarnessAdapter</span>.
              </div>
            </div>
            <div style={{ background: "var(--border)" }} />
            <div style={{ padding: 12, display: "flex", flexDirection: "column", gap: 8 }}>
              <div className="pv-row">
                <span className="pv-avatar">CX</span>
                <b className="pv-small">Codex</b>
                <span className="pv-badge is-warning">needs you</span>
                <span className="pv-rail-spacer" />
                <span className="pv-faint pv-small">1m 03s</span>
              </div>
              <div className="pv-small pv-muted">Waiting on approval to run the test suite.</div>
              <div className="pv-row">
                <button type="button" className="pv-btn sz-sm is-primary">
                  Allow
                </button>
                <button type="button" className="pv-btn sz-sm">
                  Deny
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div className="pv-block">
        <div className="pv-block-head">
          <h2>Status bar</h2>
          <span>the always-on channel: branch, harness, tokens, sync</span>
        </div>
        <div className="pv-card">
          <div className="pv-statusbar" style={{ borderTop: 0 }}>
            <span>⑂ grove-project-setup</span>
            <span>◆ 5 skills attached</span>
            <span>◈ 12 specs</span>
            <span className="pv-rail-spacer" style={{ flex: 1 }} />
            <span>18.4k tokens</span>
            <span className="pv-status-dot is-ok" />
          </div>
        </div>
      </div>
    </div>
  );
}
