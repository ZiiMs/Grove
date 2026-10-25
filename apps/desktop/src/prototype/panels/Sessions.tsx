/**
 * PROTOTYPE — throwaway. Sessions panel, three structurally different layouts over one fixture set.
 */

import { diff, sessionList, thread, type Turn } from "../fixtures";
import type { VariantDef } from "./types";

function DiffBlock({ compact = false }: { compact?: boolean }) {
  return (
    <div className="pv-diff">
      <div className="pv-diff-head">
        <span className="pv-badge">edit</span>
        <b className="pv-mono">{diff.path}</b>
        <span className="pv-rail-spacer" style={{ flex: 1 }} />
        <span style={{ color: "var(--success)" }}>+{diff.added}</span>
        <span style={{ color: "var(--danger)" }}>−{diff.removed}</span>
      </div>
      {(compact ? diff.rows.slice(0, 5) : diff.rows).map((row) => (
        <div key={`${row.kind}-${row.ln}-${row.text}`} className={`pv-diff-row is-${row.kind}`}>
          <span className="ln">{row.ln || ""}</span>
          <span className="tx">
            {row.kind === "add" ? "+" : row.kind === "del" ? "−" : " "}
            {row.text}
          </span>
        </div>
      ))}
    </div>
  );
}

function ToolCard({ turn }: { turn: Turn }) {
  if (!turn.tool) return null;
  return (
    <div className="pv-tool">
      <div className="pv-tool-head">
        <span className="pv-badge is-accent">{turn.tool.name}</span>
        <b>{turn.tool.arg}</b>
        <span className="pv-rail-spacer" style={{ flex: 1 }} />
        <span className="pv-faint">{turn.tool.state === "ok" ? "42ms" : "running"}</span>
      </div>
      <div className="pv-tool-body">{turn.tool.result}</div>
    </div>
  );
}

function Approval() {
  return (
    <div className="pv-approval">
      <div className="pv-approval-head">
        <span className="pv-status-dot is-warn" /> Needs your say-so
      </div>
      <div className="pv-small pv-muted">
        Run <span className="pv-mono">cargo test -p grove-core -- adapter</span> — 34 tests.
      </div>
      <div className="pv-row">
        <button type="button" className="pv-btn sz-sm is-primary">
          Allow once
        </button>
        <button type="button" className="pv-btn sz-sm">
          Always allow
        </button>
        <button type="button" className="pv-btn sz-sm is-ghost">
          Deny
        </button>
      </div>
    </div>
  );
}

function Composer({ compact = false }: { compact?: boolean }) {
  return (
    <div className="pv-composer">
      <textarea className="pv-composer-input" placeholder="Ask Claude Code to do something…" rows={compact ? 1 : 2} />
      <div className="pv-composer-tools">
        <button type="button" className="pv-btn sz-sm is-ghost">
          ⊕ Attach
        </button>
        <button type="button" className="pv-btn sz-sm is-ghost">
          ✦ Skill
        </button>
        <span className="pv-rail-spacer" style={{ flex: 1 }} />
        <span className="pv-model-pick">claude-opus-4 ▾</span>
        <button type="button" className="pv-btn sz-sm is-primary">
          Send ↵
        </button>
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------- A — thread */

function ThreadLayout() {
  return (
    <div className="pv-thread-col">
      <div className="pv-row">
        <span className="pv-avatar">CC</span>
        <b>Claude Code</b>
        <span className="pv-badge is-accent">opus</span>
        <span className="pv-status-dot is-live" />
        <span className="pv-rail-spacer" style={{ flex: 1 }} />
        <span className="pv-faint pv-small">4m 12s · 18.4k tokens</span>
      </div>

      {thread.map((turn) => (
        <div key={turn.id} className={turn.role === "user" ? "pv-turn is-user" : "pv-turn is-agent"}>
          <span className="pv-avatar">{turn.glyph}</span>
          <div className="pv-turn-main">
            <div className="pv-turn-name">
              {turn.harness} · {turn.time}
            </div>
            {turn.thinking ? <div className="pv-thinking">{turn.thinking}</div> : null}
            {turn.text ? (
              <div className="pv-bubble">
                {turn.text}
                {turn.streaming ? <span className="pv-stream-caret" /> : null}
              </div>
            ) : null}
            <ToolCard turn={turn} />
            {turn.id === "t3" ? <DiffBlock /> : null}
            {turn.approval ? <Approval /> : null}
            {turn.error ? (
              <div className="pv-errorstate">
                <span style={{ color: "var(--danger)" }}>✕</span>
                <div>
                  <div style={{ fontWeight: 600 }}>{turn.error.title}</div>
                  <div className="pv-faint pv-small">{turn.error.detail}</div>
                </div>
                <span className="pv-rail-spacer" style={{ flex: 1 }} />
                <button type="button" className="pv-btn sz-sm">
                  Retry
                </button>
              </div>
            ) : null}
          </div>
        </div>
      ))}

      <div className="pv-dock">
        <Composer />
      </div>
    </div>
  );
}

/* ------------------------------------------------------------ B — review desk */

function DeskLayout() {
  return (
    <div className="pv-desk">
      <section className="pv-pane">
        <div className="pv-pane-head">
          <span className="pv-status-dot is-live" />
          <b>Claude Code</b>
          <span className="pv-faint pv-small">4m 12s</span>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <span className="pv-badge is-warning">1 approval</span>
        </div>
        <div className="pv-scroll pv-col" style={{ gap: 10, padding: "var(--pad-x)" }}>
          {thread
            .filter((turn) => turn.role === "user" || turn.text)
            .map((turn) => (
              <div key={turn.id} className="pv-desk-turn">
                <div className="pv-turn-name">
                  <span className="pv-avatar">{turn.glyph}</span>
                  {turn.harness} · {turn.time}
                </div>
                <div className="pv-small">{turn.text}</div>
              </div>
            ))}
          <Approval />
          <Composer compact />
        </div>
      </section>

      <section className="pv-pane">
        <div className="pv-pane-head">
          <b>Changes</b>
          <span className="pv-badge">4 files</span>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <button type="button" className="pv-btn sz-sm">Discard</button>
          <button type="button" className="pv-btn sz-sm is-primary">Open PR</button>
        </div>
        <div className="pv-scroll pv-col" style={{ gap: 10, padding: "var(--pad-x)" }}>
          <DiffBlock />
          <div className="pv-card">
            <div className="pv-card-head">
              <b>Changed files</b>
            </div>
            {[
              { path: "adapters/acp.rs", added: 41, removed: 0 },
              { path: "adapters/mod.rs", added: 3, removed: 1 },
              { path: "contract/fixtures/session.json", added: 12, removed: 0 },
            ].map((file) => (
              <div key={file.path} className="pv-table-row" style={{ gridTemplateColumns: "1fr 52px 40px" }}>
                <span className="pv-mono">{file.path}</span>
                <span style={{ color: "var(--success)" }}>+{file.added}</span>
                <span style={{ color: "var(--danger)" }}>−{file.removed}</span>
              </div>
            ))}
          </div>
        </div>
      </section>
    </div>
  );
}

/* -------------------------------------------------------------- C — workspace */

function WorkspaceLayout() {
  return (
    <div className="pv-frame">
      <div className="pv-tabstrip">
        {sessionList.map((session) => (
          <button
            key={session.id}
            type="button"
            className={session.id === "s1" ? "pv-tab is-active" : "pv-tab"}
          >
            <span className={`pv-status-dot ${session.state === "live" ? "is-live" : "is-" + session.state}`} />
            {session.title}
            <span className="pv-x">✕</span>
          </button>
        ))}
        <button type="button" className="pv-btn sz-sm is-ghost" style={{ marginLeft: 4 }}>
          + Session
        </button>
      </div>

      <div className="pv-ws-body">
        <div className="pv-scroll pv-col" style={{ gap: 10, padding: "var(--pad-x)" }}>
          {thread
            .filter((turn) => turn.text)
            .map((turn) => (
              <div key={turn.id} className="pv-card">
                <div className="pv-card-head">
                  <span className="pv-avatar">{turn.glyph}</span>
                  <b>{turn.harness}</b>
                  <span className="pv-faint">{turn.time}</span>
                  <span className="pv-rail-spacer" style={{ flex: 1 }} />
                  <span className="pv-small pv-faint">⧉ fork</span>
                </div>
                <div className="pv-card-body">
                  <div className="pv-small">{turn.text}</div>
                  {turn.tool ? <ToolCard turn={turn} /> : null}
                </div>
              </div>
            ))}
        </div>

        <aside className="pv-side">
          <div className="pv-card">
            <div className="pv-card-head">
              <b>Attached</b>
            </div>
            <div className="pv-card-body">
              <div className="pv-row">
                <span className="pv-chip">✦ code-review</span>
                <span className="pv-chip">◇ 03-adapter-interface</span>
              </div>
              <div className="pv-progress">
                <i style={{ width: "40%" }} />
              </div>
              <div className="pv-small pv-faint">Plan · 2 of 5</div>
            </div>
          </div>
          <div className="pv-card">
            <div className="pv-card-head">
              <b>Other sessions</b>
            </div>
            {sessionList.slice(1).map((session) => (
              <div key={session.id} className="pv-table-row" style={{ gridTemplateColumns: "10px 1fr 60px" }}>
                <span className={`pv-status-dot ${session.state === "live" ? "is-live" : "is-" + session.state}`} />
                <span className="pv-small">{session.title}</span>
                <span className="pv-faint pv-small">{session.elapsed}</span>
              </div>
            ))}
          </div>
          <Composer compact />
        </aside>
      </div>
    </div>
  );
}

export const sessionsVariants: VariantDef[] = [
  { key: "A", name: "Thread", chosen: true, bet: "One column, composer docked — the classic reading order, least new to learn.", Component: ThreadLayout },
  { key: "B", name: "Review desk", bet: "Transcript and diff side by side — the primary act is landing the change, not chatting.", Component: DeskLayout },
  { key: "C", name: "Workspace", bet: "Sessions as documents in tabs — built for running four agents at once.", Component: WorkspaceLayout },
];
