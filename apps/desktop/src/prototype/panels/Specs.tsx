/**
 * PROTOTYPE — throwaway. Specs panel: graph+reader vs document-first vs canvas.
 */

import { specDoc, specs } from "../fixtures";
import type { VariantDef } from "./types";

function TreeNode({ active }: { active?: boolean }) {
  return (
    <div className="pv-tree" style={{ padding: 8 }}>
      {specs.map((node) => (
        <div
          key={node.id}
          className={node.id === "arch" || active ? "pv-tree-row is-active" : "pv-tree-row"}
          style={{ paddingLeft: 8 + node.depth * 14 }}
        >
          <span className="pv-faint">{node.type === "arch" ? "◈" : "◇"}</span>
          <span className="pv-tree-name">{node.name}</span>
          {node.edge ? <span className="pv-edge">─ {node.edge}</span> : null}
        </div>
      ))}
    </div>
  );
}

function DocBody() {
  return (
    <div className="pv-col" style={{ gap: 10 }}>
      <div className="pv-row">
        <span className="pv-badge is-accent">{specDoc.status}</span>
        <span className="pv-badge">module</span>
        <span className="pv-faint pv-small pv-mono">05-session-persistence</span>
      </div>
      <h1 className="pv-title">{specDoc.title}</h1>
      {specDoc.body.map((paragraph) => (
        <p key={paragraph.slice(0, 24)} className="pv-body pv-muted">
          {paragraph}
        </p>
      ))}
      <div className="pv-row">
        {specDoc.links.map((link) => (
          <span key={link} className="pv-chip pv-mono">
            {link}
          </span>
        ))}
      </div>
    </div>
  );
}

/* ------------------------------------------------------- A — graph + reader */

function GraphReaderLayout() {
  return (
    <div className="pv-master">
      <div className="pv-pane">
        <div className="pv-pane-head">
          <b>Specs</b>
          <span className="pv-badge">12</span>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <span className="pv-faint">⌕</span>
        </div>
        <div className="pv-scroll">
          <TreeNode />
        </div>
      </div>
      <div className="pv-pane">
        <div className="pv-pane-head">
          <span className="pv-faint">◈</span>
          <b>05-session-persistence</b>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <span className="pv-badge is-accent">pinned</span>
          <button type="button" className="pv-btn sz-sm">
            Edit
          </button>
        </div>
        <div className="pv-scroll" style={{ padding: "var(--pad-x)" }}>
          <DocBody />
        </div>
      </div>
    </div>
  );
}

/* ---------------------------------------------------- B — document first */

function DocumentLayout() {
  return (
    <div className="pv-doc">
      <div className="pv-row">
        <span className="pv-caps">specs / 05</span>
        <span className="pv-rail-spacer" style={{ flex: 1 }} />
        <span className="pv-badge is-accent">pinned to session</span>
        <button type="button" className="pv-btn sz-sm">Edit</button>
      </div>

      <DocBody />

      <div className="pv-margin">
        <span className="pv-caps">linked from</span>
        <button type="button" className="pv-link" style={{ background: "none", border: "none", textAlign: "left", padding: 0 }}>
          architecture.md §session persistence
        </button>
        <button type="button" className="pv-link" style={{ background: "none", border: "none", textAlign: "left", padding: 0 }}>
          06-panel-seam — replays the event log
        </button>
      </div>

      <div className="pv-card">
        <div className="pv-card-head">
          <b>Neighbourhood</b>
          <span className="pv-faint">the graph, inline — no separate panel to visit</span>
        </div>
        <div className="pv-card-body">
          <div className="pv-row">
            <span className="pv-chip">↑ architecture.md</span>
            <span className="pv-chip">→ 04-ipc-contract</span>
            <span className="pv-chip">← 03-adapter-interface</span>
            <span className="pv-chip">↳ contract-fixtures</span>
          </div>
        </div>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------ C — canvas */

const nodes = [
  { id: "goal", x: 24, y: 26, label: "goal-and-requirements", type: "goal" },
  { id: "arch", x: 250, y: 26, label: "architecture", type: "arch" },
  { id: "03", x: 24, y: 132, label: "03-adapter-interface", type: "module" },
  { id: "04", x: 250, y: 132, label: "04-ipc-contract", type: "module" },
  { id: "05", x: 24, y: 238, label: "05-session-persistence", type: "module" },
  { id: "06", x: 250, y: 238, label: "06-panel-seam", type: "module" },
  { id: "fx", x: 476, y: 132, label: "contract-fixtures", type: "sub" },
];

const edges: [string, string][] = [
  ["goal", "arch"],
  ["arch", "03"],
  ["arch", "04"],
  ["04", "fx"],
  ["arch", "05"],
  ["arch", "06"],
  ["03", "05"],
];

function CanvasLayout() {
  const byId = new Map(nodes.map((node) => [node.id, node]));
  return (
    <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
      <div className="pv-row">
        <span className="pv-badge is-accent">graph</span>
        <span className="pv-small pv-faint">click a node → inspector · 12 specs, 19 edges</span>
        <span className="pv-rail-spacer" style={{ flex: 1 }} />
        <div className="pv-segmented">
          <button type="button" className="pv-seg is-active">Graph</button>
          <button type="button" className="pv-seg">Outline</button>
        </div>
      </div>

      <div className="pv-canvas">
        <svg className="pv-canvas-edges" width="100%" height="100%">
          {edges.map(([from, to]) => {
            const a = byId.get(from);
            const b = byId.get(to);
            if (!a || !b) return null;
            return (
              <line
                key={`${from}-${to}`}
                x1={a.x + 86}
                y1={a.y + 52}
                x2={b.x + 86}
                y2={b.y}
                stroke="var(--border-strong)"
                strokeWidth="1"
              />
            );
          })}
        </svg>
        {nodes.map((node) => (
          <div
            key={node.id}
            className={node.id === "05" ? "pv-node is-active" : "pv-node"}
            style={{ left: node.x, top: node.y }}
          >
            <span className="pv-caps">{node.type}</span>
            <b className="pv-mono">{node.label}</b>
          </div>
        ))}
      </div>

      <div className="pv-card">
        <div className="pv-card-head">
          <b>Inspector</b>
          <span className="pv-mono">05-session-persistence</span>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <button type="button" className="pv-btn sz-sm is-primary">Pin to session</button>
        </div>
        <div className="pv-card-body">
          <p className="pv-body pv-muted">{specDoc.body[0]}</p>
          <div className="pv-row">
            {specDoc.links.map((link) => (
              <span key={link} className="pv-chip pv-mono">{link}</span>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

export const specsVariants: VariantDef[] = [
  { key: "A", name: "Graph + reader", bet: "Tree on the left, document on the right — the IDE convention, learned in five seconds.", Component: GraphReaderLayout },
  { key: "B", name: "Document first", bet: "The spec is the page; links become margin notes. Best for actually reading, worst for browsing 40 specs.", Component: DocumentLayout },
  { key: "C", name: "Canvas", chosen: true, bet: "Node-link overview with an inspector — shows shape and orphans, which a tree cannot.", Component: CanvasLayout },
];
