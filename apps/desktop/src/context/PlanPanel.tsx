import { useContextStore } from "../state/contextStore";
import { Icon } from "../workspace/Icon";

interface PlanItem {
  text: string;
  state: "done" | "active" | "todo";
}

/** Read the agent-editable markdown as a checklist. The plan mirror is the record; this is a view. */
function parsePlan(markdown: string): PlanItem[] {
  const items: PlanItem[] = [];
  for (const line of markdown.split("\n")) {
    const match = /^\s*(?:[-*+]|\d+\.)\s+\[([ xX~>])\]\s+(.+)$/.exec(line);
    if (match) {
      const mark = match[1];
      items.push({
        text: match[2].trim(),
        state: mark === "x" || mark === "X" ? "done" : mark === "~" || mark === ">" ? "active" : "todo",
      });
      continue;
    }
    // A plan without checkboxes still reads as a list; bullets become todo rows.
    const bullet = /^\s*[-*+]\s+(.+)$/.exec(line);
    if (bullet && !bullet[1].startsWith("[ ]")) {
      items.push({ text: bullet[1].trim(), state: "todo" });
    }
  }
  if (!items.some((item) => item.state === "active")) {
    const first = items.find((item) => item.state === "todo");
    if (first) first.state = "active";
  }
  return items;
}

export function planProgress(markdown: string): { done: number; total: number } {
  const items = parsePlan(markdown);
  return { done: items.filter((item) => item.state === "done").length, total: items.length };
}

export function PlanPanel() {
  const plan = useContextStore((state) => state.plan);
  const error = useContextStore((state) => state.planError);
  const items = plan ? parsePlan(plan.markdown) : [];
  const done = items.filter((item) => item.state === "done").length;
  const percent = items.length ? Math.round((done / items.length) * 100) : 0;

  return (
    <div className="insp-pane">
      <div className="insp-scroll od-scroll">
        <div className="pane-head">
          <span id="plan-summary">
            {items.length ? `${done} of ${items.length} complete` : "No plan yet"}
          </span>
          {plan && plan.version > 0 ? <span className="ph-tail">v{plan.version}</span> : null}
        </div>
        <div className="progress-track">
          <div className="progress-fill" style={{ width: `${percent}%` }} />
        </div>
        {error ? (
          <p className="git-empty">{error}</p>
        ) : items.length === 0 ? (
          <p className="git-empty">The agent writes a checklist to the session's plan mirror; it will appear here.</p>
        ) : (
          <div>
            {items.map((item, index) => (
              <div className="plan-row" data-p={item.state} key={`${index}-${item.text}`}>
                <span className="p-ico" data-p={item.state}>
                  <Icon
                    name={
                      item.state === "done"
                        ? "checkCircle"
                        : item.state === "active"
                          ? "clock"
                          : "circle"
                    }
                    size="sm"
                  />
                </span>
                <span className="p-text">{item.text}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
