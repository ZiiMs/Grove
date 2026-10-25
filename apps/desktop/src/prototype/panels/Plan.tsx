/**
 * PROTOTYPE — throwaway. Plan panel: checklist vs lanes vs timeline.
 */

import { plan } from "../fixtures";
import type { VariantDef } from "./types";

const done = plan.filter((item) => item.state === "done").length;
const pct = Math.round((done / plan.length) * 100);

/* ------------------------------------------------------------ A — checklist */

function ChecklistLayout() {
  return (
    <div style={{ maxWidth: 640, width: "100%", margin: "0 auto" }}>
      <div className="pv-card">
        <div className="pv-card-head">
          <b>Plan</b>
          <span className="pv-badge is-accent">{done}/{plan.length}</span>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <span className="pv-faint pv-small">mirrored to .grove/plan.md</span>
          <button type="button" className="pv-btn sz-sm">+ Step</button>
        </div>
        <div className="pv-card-body">
          <div className="pv-progress"><i style={{ width: `${pct}%` }} /></div>
          <div className="pv-todo">
            {plan.map((item) => (
              <div
                key={item.id}
                className={`pv-todo-row${item.state === "done" ? " is-done" : ""}${item.state === "now" ? " is-active" : ""}`}
              >
                <label className="pv-check">
                  <input type="checkbox" defaultChecked={item.state === "done"} />
                </label>
                <span className="pv-todo-text">{item.text}</span>
                {item.state === "now" ? <span className="pv-badge is-accent">now</span> : null}
                {item.state === "blocked" ? <span className="pv-badge is-danger">blocked</span> : null}
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------- B — lanes */

function LanesLayout() {
  const lanes = [
    { id: "todo", title: "To do", items: plan.filter((item) => item.state === "todo") },
    { id: "now", title: "Doing now", items: plan.filter((item) => item.state === "now") },
    { id: "done", title: "Done", items: plan.filter((item) => item.state === "done" || item.state === "blocked") },
  ];
  return (
    <div className="pv-lanes">
      {lanes.map((lane) => (
        <div key={lane.id} className="pv-lane">
          <div className="pv-lane-head">
            <b>{lane.title}</b>
            <span className="pv-badge">{lane.items.length}</span>
            <span className="pv-rail-spacer" style={{ flex: 1 }} />
            <span className="pv-faint">＋</span>
          </div>
          <div className="pv-lane-body">
            {lane.items.map((item) => (
              <div key={item.id} className={item.state === "now" ? "pv-ticket is-now" : "pv-ticket"}>
                <span>{item.text}</span>
                <div className="pv-row">
                  {item.state === "blocked" ? (
                    <span className="pv-badge is-danger">blocked</span>
                  ) : item.state === "done" ? (
                    <span className="pv-badge is-success">done</span>
                  ) : null}
                  <span className="pv-rail-spacer" style={{ flex: 1 }} />
                  <span className="pv-avatar" style={{ width: 18, height: 18, fontSize: 8 }}>CC</span>
                </div>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

/* ------------------------------------------------------------- C — timeline */

function TimelineLayout() {
  return (
    <div className="pv-card">
      <div className="pv-card-head">
        <b>Ticket order</b>
        <span className="pv-faint">blocking edges as order, not as prose</span>
        <span className="pv-rail-spacer" style={{ flex: 1 }} />
        <span className="pv-badge">{done}/{plan.length} closed</span>
      </div>
      <div className="pv-card-body">
        <div className="pv-timeline">
          {plan.map((item, index) => (
            <div key={item.id} className="pv-timeline-row">
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
              >
                {item.state === "done" ? "✓" : item.state === "blocked" ? "!" : index + 1}
              </span>
              <div className="pv-col" style={{ gap: 2 }}>
                <span className="pv-body">{item.text}</span>
                <span className="pv-faint pv-small pv-mono">
                  {item.state === "blocked" ? "blocked by 04-ipc-contract" : `ticket ${String(index + 1).padStart(2, "0")}`}
                </span>
              </div>
              <span className={item.state === "now" ? "pv-badge is-accent" : "pv-badge"}>{item.state}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

export const planVariants: VariantDef[] = [
  { key: "A", name: "Checklist", bet: "A single ordered list with a progress bar — the plan as the agent reads it.", Component: ChecklistLayout },
  { key: "B", name: "Lanes", bet: "Todo / now / done columns — makes WIP limits and stalled work visible at a glance.", Component: LanesLayout },
  { key: "C", name: "Timeline", chosen: true, bet: "Numbered tickets with blocking edges — the plan as a dependency order.", Component: TimelineLayout },
];
