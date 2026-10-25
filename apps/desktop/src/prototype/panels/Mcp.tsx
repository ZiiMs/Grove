/**
 * PROTOTYPE — throwaway. MCP panel: table vs server cards vs inspector.
 */

import { mcpServers } from "../fixtures";
import type { VariantDef } from "./types";

function dot(state: string) {
  return `pv-status-dot is-${state}`;
}

/* ------------------------------------------------------------------ A — table */

function TableLayout() {
  return (
    <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
      <div className="pv-row">
        <span className="pv-badge is-success">3 connected</span>
        <span className="pv-badge is-warning">1 degraded</span>
        <span className="pv-badge is-danger">1 failed</span>
        <span className="pv-rail-spacer" style={{ flex: 1 }} />
        <button type="button" className="pv-btn sz-sm">+ Add server</button>
      </div>
      <div className="pv-card">
        <div className="pv-table cols-mcp">
          <div className="pv-table-head">
            <span />
            <span>server</span>
            <span>transport</span>
            <span>tools</span>
            <span>state</span>
          </div>
          {mcpServers.map((server) => (
            <div key={server.name} className="pv-table-row">
              <span className={dot(server.state)} />
              <span className="pv-mono">{server.name}</span>
              <span className="pv-small pv-faint">{server.transport}</span>
              <span>{server.tools.length}</span>
              <span className="pv-small pv-faint">{server.state}</span>
            </div>
          ))}
        </div>
      </div>
      <div className="pv-small pv-faint">
        The table is the cheapest view and the least useful once a server has 20 tools.
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ B — cards */

function CardsLayout() {
  return (
    <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
      <div className="pv-row">
        <div className="pv-input-wrap" style={{ flex: 1, maxWidth: 320 }}>
          <span className="pv-faint">⌕</span>
          <input className="pv-input" placeholder="Filter servers and tools…" />
        </div>
        <span className="pv-rail-spacer" style={{ flex: 1 }} />
        <button type="button" className="pv-btn sz-sm is-primary">+ Add server</button>
      </div>
      <div className="pv-skill-grid" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(268px, 1fr))" }}>
        {mcpServers.map((server) => (
          <div key={server.name} className="pv-card">
            <div className="pv-card-head">
              <span className={dot(server.state)} />
              <b className="pv-mono">{server.name}</b>
              <span className="pv-rail-spacer" style={{ flex: 1 }} />
              <label className="pv-switch" aria-label={`Enable ${server.name}`}>
                <input type="checkbox" defaultChecked={server.state === "ok"} />
              </label>
            </div>
            <div className="pv-card-body">
              <div className="pv-row">
                <span className="pv-badge">{server.transport}</span>
                <span className="pv-badge">{server.tools.length} tools</span>
              </div>
              <div className="pv-row" style={{ gap: 4 }}>
                {server.tools.slice(0, 4).map((tool) => (
                  <span key={tool} className="pv-badge pv-mono">{tool}</span>
                ))}
                {server.tools.length > 4 ? <span className="pv-faint pv-small">+{server.tools.length - 4}</span> : null}
                {server.tools.length === 0 ? <span className="pv-small" style={{ color: "var(--danger)" }}>no tools — connection failed</span> : null}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

/* -------------------------------------------------------------- C — inspector */

function InspectorLayout() {
  const selected = mcpServers[1];
  return (
    <div className="pv-master is-narrow">
      <div className="pv-pane">
        <div className="pv-pane-head">
          <b>Servers</b>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <span className="pv-faint">＋</span>
        </div>
        <div className="pv-scroll pv-tree" style={{ padding: 6 }}>
          {mcpServers.map((server) => (
            <div key={server.name} className={server.name === selected.name ? "pv-tree-row is-active" : "pv-tree-row"}>
              <span className={dot(server.state)} />
              <span className="pv-tree-name pv-mono">{server.name}</span>
              <span className="pv-faint pv-small">{server.tools.length}</span>
            </div>
          ))}
        </div>
      </div>
      <div className="pv-pane">
        <div className="pv-pane-head">
          <span className={dot(selected.state)} />
          <b className="pv-mono">{selected.name}</b>
          <span className="pv-badge">{selected.transport}</span>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <button type="button" className="pv-btn sz-sm">Reconnect</button>
          <button type="button" className="pv-btn sz-sm is-danger">Remove</button>
        </div>
        <div className="pv-scroll">
          <div className="pv-table-head" style={{ gridTemplateColumns: "1fr 90px" }}>
            <span>tool</span>
            <span>enabled</span>
          </div>
          <div className="pv-tool-list">
            {selected.tools.map((tool) => (
              <div key={tool} className="pv-tool-row">
                <span className="pv-mono">{tool}</span>
                <span className="pv-rail-spacer" style={{ flex: 1 }} />
                <label className="pv-switch" aria-label={`Enable ${tool}`}>
                  <input type="checkbox" defaultChecked />
                </label>
              </div>
            ))}
          </div>
          <div style={{ padding: "var(--pad-x)" }}>
            <div className="pv-errorstate">
              <span style={{ color: "var(--danger)" }}>✕</span>
              <div>
                <div style={{ fontWeight: 600 }}>sentry — SSE stream closed</div>
                <div className="pv-faint pv-small">Last connected 14:01 · 0 tools exposed</div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

export const mcpVariants: VariantDef[] = [
  { key: "A", name: "Table", bet: "One row per server — dense, scannable, and enough when tools are not the question.", Component: TableLayout },
  { key: "B", name: "Server cards", bet: "A card per server with its tool chips inline — the shape is the server, not the row.", Component: CardsLayout },
  { key: "C", name: "Inspector", chosen: true, bet: "Servers left, tools right with per-tool toggles — the only layout that scales past 20 tools.", Component: InspectorLayout },
];
