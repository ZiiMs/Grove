import type { Project, SessionDetail } from "@grove/contract";
import { useEffect } from "react";
import { useBridge } from "../ipc/context";
import { useContextStore } from "../state/contextStore";
import { useSessionStore } from "../state/sessionStore";
import { useUiStore, type ContextTab } from "../state/uiStore";
import { Icon } from "../workspace/Icon";
import { GitPanel } from "./GitPanel";
import { PlanPanel, planProgress } from "./PlanPanel";
import { SpecPanel } from "./SpecPanel";

interface ContextPanelProps {
  active: boolean;
  session: SessionDetail | null;
  project: Project | null;
}

const TABS: { id: ContextTab; label: string }[] = [
  { id: "git", label: "Git" },
  { id: "plan", label: "Plan" },
  { id: "spec", label: "Spec" },
];

/** The right column: per-session context. Its tabs never navigate away from the chat. */
export function ContextPanel({ active, session, project }: ContextPanelProps) {
  const tab = useUiStore((state) => state.contextTab);
  const setTab = useUiStore((state) => state.setContextTab);
  const setOpen = useUiStore((state) => state.setContextOpen);
  const bridge = useBridge();
  const git = useContextStore((state) => state.git);
  const plan = useContextStore((state) => state.plan);
  const activePath = useContextStore((state) => state.activePath);
  const refreshGit = useContextStore((state) => state.refreshGit);
  const refreshPlan = useContextStore((state) => state.refreshPlan);
  const refreshSpecs = useContextStore((state) => state.refreshSpecs);
  const activeId = useSessionStore((state) => state.activeId);
  const progress = plan ? planProgress(plan.markdown) : { done: 0, total: 0 };
  const fileCount = git?.files.length ?? 0;

  // A tab is a view over live state: opening it re-reads, so an agent edit made while the tab was
  // closed shows up without waiting for an event.
  useEffect(() => {
    if (tab === "git") void refreshGit(bridge, activePath);
    else if (tab === "plan" && activeId) void refreshPlan(bridge, activeId);
    else if (tab === "spec") void refreshSpecs(bridge);
  }, [activeId, activePath, bridge, refreshGit, refreshPlan, refreshSpecs, tab]);

  return (
    <aside
      className={active ? "panel context is-active" : "panel context"}
      id="panel-context"
      aria-label="Context panel"
    >
      <div className="ctx-head">
        <div className="tabs" role="tablist" aria-label="Context view">
          {TABS.map((entry) => (
            <button
              key={entry.id}
              type="button"
              role="tab"
              aria-selected={tab === entry.id}
              aria-controls={`insp-${entry.id}`}
              onClick={() => setTab(entry.id)}
            >
              {entry.label}
              {entry.id === "git" && fileCount > 0 ? <span className="badge">{fileCount}</span> : null}
              {entry.id === "plan" && progress.total > 0 ? (
                <span className="badge">
                  {progress.done}/{progress.total}
                </span>
              ) : null}
            </button>
          ))}
        </div>
        <button
          className="icon-btn sm"
          type="button"
          aria-label="Hide context panel"
          onClick={() => setOpen(false)}
        >
          <Icon name="chevronRight" size="sm" />
        </button>
      </div>

      {tab === "git" ? (
        <div className="insp-tab" id="insp-git" role="tabpanel" aria-label="Working tree changes">
          <GitPanel session={session} project={project} />
        </div>
      ) : null}
      {tab === "plan" ? (
        <div className="insp-tab" id="insp-plan" role="tabpanel" aria-label="Plan">
          <PlanPanel />
        </div>
      ) : null}
      {tab === "spec" ? (
        <div className="insp-tab" id="insp-spec" role="tabpanel" aria-label="Specs">
          <SpecPanel />
        </div>
      ) : null}
    </aside>
  );
}
