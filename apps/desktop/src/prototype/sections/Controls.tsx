/**
 * PROTOTYPE — throwaway. Controls, inputs, type scale, badges — every variant we might use.
 */

export function ControlsSection() {
  return (
    <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
      <div className="pv-block">
        <div className="pv-block-head">
          <h2>Buttons</h2>
          <span>variants × sizes × states</span>
        </div>
        <div className="pv-card">
          <div className="pv-card-body">
            <div className="pv-row">
              <button type="button" className="pv-btn is-primary">
                New session
              </button>
              <button type="button" className="pv-btn">
                Install skill
              </button>
              <button type="button" className="pv-btn is-outline">
                Open PR
              </button>
              <button type="button" className="pv-btn is-ghost">
                Cancel
              </button>
              <button type="button" className="pv-btn is-danger">
                Delete plan
              </button>
              <button type="button" className="pv-btn is-icon" aria-label="Refresh">
                ⟳
              </button>
              <div className="pv-split">
                <button type="button" className="pv-btn is-primary">
                  Commit &amp; push
                </button>
                <button type="button" className="pv-btn is-primary" aria-label="More">
                  ▾
                </button>
              </div>
            </div>
            <div className="pv-row">
              <button type="button" className="pv-btn sz-sm">
                Small
              </button>
              <button type="button" className="pv-btn">Medium</button>
              <button type="button" className="pv-btn sz-lg is-primary">
                Large primary
              </button>
              <button type="button" className="pv-btn is-primary is-loading">
                Working
              </button>
              <button type="button" className="pv-btn" disabled>
                Disabled
              </button>
              <button type="button" className="pv-btn is-primary" disabled>
                Disabled primary
              </button>
            </div>
            <div className="pv-row">
              <span className="pv-faint pv-small">focus ring →</span>
              <button type="button" className="pv-btn" style={{ outline: "2px solid var(--accent)", outlineOffset: 1 }}>
                Focused
              </button>
              <button type="button" className="pv-btn is-ghost sz-sm">
                ⧉ Copy
              </button>
              <button type="button" className="pv-btn sz-sm">
                ✓ Approve
              </button>
              <button type="button" className="pv-btn sz-sm">
                ✕ Deny
              </button>
            </div>
          </div>
        </div>
      </div>

      <div className="pv-grid-2">
        <div className="pv-block">
          <div className="pv-block-head">
            <h2>Inputs</h2>
          </div>
          <div className="pv-card">
            <div className="pv-card-body">
              <div className="pv-field">
                <label className="pv-label" htmlFor="pv-f1">
                  Session name
                </label>
                <input id="pv-f1" className="pv-input" defaultValue="ACP adapter behind HarnessAdapter" />
              </div>
              <div className="pv-field">
                <span className="pv-label">Search</span>
                <div className="pv-input-wrap">
                  <span className="pv-faint">⌕</span>
                  <input className="pv-input" placeholder="Filter skills…" />
                  <span className="pv-kbd">/</span>
                </div>
              </div>
              <div className="pv-field">
                <span className="pv-label">
                  Install location <span className="pv-badge">native dir</span>
                </span>
                <select className="pv-select" defaultValue="claude">
                  <option value="claude">.claude/skills — project</option>
                  <option value="claude-user">~/.claude/skills — user</option>
                  <option value="codex">.codex/skills — project</option>
                  <option value="pi">~/.pi/skills — user</option>
                </select>
              </div>
              <div className="pv-field">
                <span className="pv-label">Prompt</span>
                <textarea className="pv-textarea" rows={3} defaultValue="Explain why the adapter seam is one trait." />
              </div>
              <div className="pv-field">
                <span className="pv-label">Error state</span>
                <input
                  className="pv-input"
                  style={{ borderColor: "var(--danger)", boxShadow: "0 0 0 3px var(--danger-soft)" }}
                  defaultValue="not-a-valid-path"
                />
                <span className="pv-small" style={{ color: "var(--danger)" }}>
                  Directory does not exist.
                </span>
              </div>
            </div>
          </div>
        </div>

        <div className="pv-block">
          <div className="pv-block-head">
            <h2>Selection & toggles</h2>
          </div>
          <div className="pv-card">
            <div className="pv-card-body">
              <div className="pv-col">
                <label className="pv-check">
                  <input type="checkbox" defaultChecked /> Write into each harness&apos;s own dir
                </label>
                <label className="pv-check">
                  <input type="checkbox" /> Also mirror into project skills
                </label>
              </div>
              <div className="pv-col">
                <label className="pv-check is-radio">
                  <input type="radio" name="pv-r" defaultChecked /> Pin spec to session
                </label>
                <label className="pv-check is-radio">
                  <input type="radio" name="pv-r" /> Reference only
                </label>
              </div>
              <div className="pv-col">
                <label className="pv-switch">
                  <input type="checkbox" defaultChecked /> Stream tool calls as cards
                </label>
                <label className="pv-switch">
                  <input type="checkbox" /> Auto-approve file reads
                </label>
              </div>
              <div className="pv-field">
                <span className="pv-label">
                  Context budget <span className="pv-faint pv-small">18.4k / 64k</span>
                </span>
                <input className="pv-slider" type="range" defaultValue={30} />
              </div>
              <div className="pv-row">
                <div className="pv-segmented">
                  <button type="button" className="pv-seg">
                    Compact
                  </button>
                  <button type="button" className="pv-seg is-active">
                    Cozy
                  </button>
                  <button type="button" className="pv-seg">
                    Comfy
                  </button>
                </div>
                <span className="pv-faint pv-small">density</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div className="pv-grid-2">
        <div className="pv-block">
          <div className="pv-block-head">
            <h2>Badges, chips, keys</h2>
          </div>
          <div className="pv-card">
            <div className="pv-card-body">
              <div className="pv-row">
                <span className="pv-badge">default</span>
                <span className="pv-badge is-accent">adapter</span>
                <span className="pv-badge is-success">passing</span>
                <span className="pv-badge is-warning">needs you</span>
                <span className="pv-badge is-danger">failing</span>
              </div>
              <div className="pv-row">
                <span className="pv-chip">
                  <span className="pv-status-dot is-ok" /> claude-code
                </span>
                <span className="pv-chip">
                  <span className="pv-status-dot is-live" /> codex
                </span>
                <span className="pv-chip">
                  <span className="pv-status-dot is-warn" /> opencode
                </span>
                <span className="pv-chip">
                  <span className="pv-status-dot" /> pi
                </span>
              </div>
              <div className="pv-row">
                <span className="pv-chip">
                  code-review <span className="pv-x">✕</span>
                </span>
                <span className="pv-chip">
                  brainstorming <span className="pv-x">✕</span>
                </span>
                <span className="pv-chip">
                  + attach skill
                </span>
              </div>
              <div className="pv-row">
                <span className="pv-kbd">⌘</span>
                <span className="pv-kbd">K</span>
                <span className="pv-faint pv-small">command palette</span>
                <span className="pv-kbd">⇧</span>
                <span className="pv-kbd">↵</span>
                <span className="pv-faint pv-small">new session</span>
              </div>
              <div className="pv-row">
                <span className="pv-avatar">CC</span>
                <span className="pv-avatar">CX</span>
                <span className="pv-avatar">AC</span>
                <span className="pv-faint pv-small">harness avatars</span>
              </div>
            </div>
          </div>
        </div>

        <div className="pv-block">
          <div className="pv-block-head">
            <h2>Type scale</h2>
          </div>
          <div className="pv-card">
            <div className="pv-card-body">
              <h1 className="pv-display">One home for every harness</h1>
              <h2 className="pv-title">Spec graph · 12 documents</h2>
              <div className="pv-caps">Section label · caps</div>
              <p className="pv-body">
                Body text at {`var(--font-md)`} — the default for transcripts, tables and prose. Line height
                1.45 keeps dense panels readable.
              </p>
              <p className="pv-small pv-muted">
                Small muted text for metadata, timestamps and helper copy under controls.
              </p>
              <div className="pv-row">
                <span className="pv-mono">pnpm tauri dev</span>
                <span className="pv-code">HarnessAdapter</span>
                <a className="pv-link" href="#prototype">
                  a link
                </a>
              </div>
              <p className="pv-small pv-faint">
                Faint text: disabled, placeholder, and de-emphasised counts.
              </p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
