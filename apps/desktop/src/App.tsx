import { useEffect, useMemo, useRef, useState } from "react";
import { useBridge } from "./ipc/context";
import {
  subscribeToGitChanged,
  subscribeToSessionEvents,
  subscribeToPlanChanged,
  subscribeToSpecChanged,
} from "./ipc/events";
import { useSessionStore } from "./state/sessionStore";
import { useContextStore } from "./state/contextStore";
import { useMenuStore } from "./state/menuStore";
import { useUiStore } from "./state/uiStore";
import { useWorkspaceStore } from "./state/workspaceStore";
import { NavTree } from "./shell/NavTree";
import { ContextMenuHost } from "./shell/ContextMenu";
import {
  archiveSession,
  projectMenuItems,
  sessionMenuItems,
  worktreeMenuItems,
} from "./shell/menus";
import { StatusBar } from "./shell/StatusBar";
import { Titlebar } from "./shell/Titlebar";
import { SessionSurface } from "./session/SessionSurface";
import { ContextPanel } from "./context/ContextPanel";
import { SettingsModal } from "./settings/SettingsModal";
import { NewSessionDialog } from "./dialogs/NewSessionDialog";
import { AddProjectDialog } from "./dialogs/AddProjectDialog";
import { DiffModal } from "./dialogs/DiffModal";
import { PrDialog } from "./dialogs/PrDialog";
import { Toast } from "./shell/Toast";
import { Icon } from "./workspace/Icon";
import { normalizePath, projectForSession, worktreeForSession } from "./workspace/tree";
import { planProgress } from "./context/PlanPanel";
import { providerMeta } from "./workspace/models";
import { modelLabel } from "./workspace/modelLabel";

/** The production shell: titlebar, tree, always-chat centre, context column, status bar. */
export default function App() {
  const bridge = useBridge();
  const sessions = useSessionStore((state) => state.sessions);
  const activeId = useSessionStore((state) => state.activeId);
  const detail = useSessionStore((state) => state.detail);
  const events = useSessionStore((state) => state.events);
  const loading = useSessionStore((state) => state.loading);
  const load = useSessionStore((state) => state.load);
  const select = useSessionStore((state) => state.select);
  const projects = useWorkspaceStore((state) => state.projects);
  const worktrees = useWorkspaceStore((state) => state.worktrees);
  const branchesByProject = useWorkspaceStore((state) => state.branches);
  const info = useWorkspaceStore((state) => state.info);
  const prefs = useWorkspaceStore((state) => state.prefs);
  const loadWorkspace = useWorkspaceStore((state) => state.load);
  const refreshProjectGit = useWorkspaceStore((state) => state.refreshProjectGit);
  const view = useUiStore((state) => state.view);
  const setView = useUiStore((state) => state.setView);
  const notify = useUiStore((state) => state.notify);
  const openMenu = useMenuStore((state) => state.openMenu);
  const contextOpen = useUiStore((state) => state.contextOpen);
  const setContextOpen = useUiStore((state) => state.setContextOpen);
  const setContextTab = useUiStore((state) => state.setContextTab);
  const openNewSession = useUiStore((state) => state.openNewSession);
  const openAddProject = useUiStore((state) => state.openAddProject);
  const openSettings = useUiStore((state) => state.openSettings);
  const git = useContextStore((state) => state.git);
  const gitStats = useContextStore((state) => state.gitStats);
  const plan = useContextStore((state) => state.plan);
  const refreshFor = useContextStore((state) => state.refreshFor);
  const refreshSpecs = useContextStore((state) => state.refreshSpecs);
  const refreshGit = useContextStore((state) => state.refreshGit);
  const applyPlan = useContextStore((state) => state.applyPlan);
  const [filter, setFilter] = useState("");
  const autoSelected = useRef(false);

  const activeSummary = sessions.find((row) => row.id === activeId) ?? null;
  const activeProject = activeSummary ? projectForSession(activeSummary, projects) : null;
  const activeWorktrees = activeProject ? (worktrees[activeProject.id] ?? []) : [];
  const activeWorktree = activeSummary ? worktreeForSession(activeSummary, activeWorktrees) : null;
  const branches = activeProject ? (branchesByProject[activeProject.id] ?? []) : [];
  const planStats = plan ? planProgress(plan.markdown) : { done: 0, total: 0 };
  // Archived sessions are hidden from the tree; they must not summon the "needs you" badge either.
  const awaiting = sessions.filter((row) => row.awaiting && !row.archivedAt);

  useEffect(() => {
    void loadWorkspace(bridge);
    void load(bridge);
  }, [bridge, load, loadWorkspace]);

  // One subscription block for the lifetime of the shell.
  useEffect(() => {
    let active = true;
    const stops: (() => void)[] = [];
    const keep = (stop: () => void) => {
      if (active) stops.push(stop);
      else stop();
    };
    void subscribeToSessionEvents(bridge, (sessionId, event) => {
      useSessionStore.getState().applyEvent(sessionId, event);
      // The first assistant reply is the moment a blank-title session has enough material to be
      // named. `maybeGenerateTitle` guards one attempt per auto session per app run and never
      // touches a manual title.
      if (sessionId && event.kind === "message" && event.role === "assistant") {
        void useSessionStore.getState().maybeGenerateTitle(bridge, sessionId);
      }
      if (
        event.kind === "status" &&
        (event.state === "idle" || event.state === "ready" || event.state === "exited")
      ) {
        void useSessionStore.getState().flushQueue(bridge);
        // A cross-provider model change queued behind a turn lands here, once the session is idle.
        void useSessionStore
          .getState()
          .flushPendingModel(bridge)
          .then((applied) => {
            if (applied) {
              notify(
                `Session now runs on ${providerMeta(applied.provider).name} · ${modelLabel(
                  applied.model,
                  applied.provider,
                )}`,
              );
            }
          });
      }
    }).then(keep);
    void subscribeToGitChanged(bridge, (status) => {
      const current = useContextStore.getState().activePath;
      if (!current) return;
      const root = normalizePath(status.repoRoot);
      const path = normalizePath(current);
      if (path === root || path.startsWith(`${root}/`)) void refreshGit(bridge, current);
    }).then(keep);
    void subscribeToPlanChanged(bridge, (sessionId, next) => applyPlan(sessionId, next)).then(keep);
    void subscribeToSpecChanged(bridge, () => void refreshSpecs(bridge)).then(keep);
    return () => {
      active = false;
      stops.forEach((stop) => stop());
    };
  }, [applyPlan, bridge, notify, refreshGit, refreshSpecs]);

  // Restore the last conversation once the list and preferences are both here. An archived session
  // is not a launch target: it opens from the Archive section, on purpose.
  useEffect(() => {
    const restorable = sessions.filter((row) => !row.archivedAt);
    if (autoSelected.current || loading || restorable.length === 0) return;
    autoSelected.current = true;
    if (!prefs.restoreSessions) return;
    let restore: string | null = null;
    try {
      restore = localStorage.getItem("grove.lastSession");
    } catch {
      restore = null;
    }
    const id =
      restore && restorable.some((row) => row.id === restore) ? restore : restorable[0].id;
    void select(bridge, id);
    setContextTab("git");
  }, [bridge, loading, prefs.restoreSessions, select, sessions, setContextTab]);

  // The context panel follows the active session/worktree, not every streamed event.
  const activeCwd = detail?.cwd ?? null;
  useEffect(() => {
    void refreshFor(bridge, useSessionStore.getState().detail, activeProject);
  }, [activeProject, activeId, activeCwd, bridge, refreshFor]);

  useEffect(() => {
    if (activeProject) void refreshProjectGit(bridge, activeProject);
  }, [activeProject, bridge, refreshProjectGit]);

  const onSelect = (id: string) => {
    void select(bridge, id);
    setView("session");
  };

  const onGit = () => {
    setContextTab("git");
    setContextOpen(true);
    setView(window.matchMedia("(max-width: 899px)").matches ? "context" : "session");
  };

  const onAwaiting = () => {
    const next = awaiting[0];
    if (next) {
      void select(bridge, next.id);
      setView("session");
    }
  };

  const newSessionFor = (projectId: string | null, worktreePath: string | null) => {
    openNewSession({ projectId, worktreePath });
  };

  const mobileTabs = useMemo(
    () => [
      { id: "nav" as const, label: "Navigation" },
      { id: "session" as const, label: "Session" },
      { id: "context" as const, label: "Context" },
    ],
    [],
  );

  return (
    <div className="app" id="app" data-ctx={contextOpen ? "open" : "closed"}>
      <Titlebar
        project={activeProject}
        session={detail}
        git={git}
        gitStats={gitStats}
        branches={branches}
        awaitingCount={awaiting.length}
        onGit={onGit}
        onAwaiting={onAwaiting}
      />

      <nav className="mobile-nav" role="tablist" aria-label="Workspace pane">
        {mobileTabs.map((entry) => (
          <button
            key={entry.id}
            type="button"
            role="tab"
            data-view={entry.id}
            aria-controls={`panel-${entry.id}`}
            aria-selected={view === entry.id}
            onClick={() => {
              if (entry.id === "context") setContextOpen(true);
              setView(entry.id);
            }}
          >
            {entry.label}
          </button>
        ))}
      </nav>

      <div className="workspace">
        <NavTree
          active={view === "nav"}
          filter={filter}
          onFilter={setFilter}
          activeId={activeId}
          onSelect={onSelect}
          onNewSession={newSessionFor}
          onNewChat={() => newSessionFor(null, null)}
          onAddProject={openAddProject}
          onSettings={() => openSettings("general")}
          onSessionMenu={(row, anchor) =>
            openMenu({ ...anchor, items: sessionMenuItems(bridge, row) })
          }
          onProjectMenu={(project, anchor) =>
            openMenu({ ...anchor, items: projectMenuItems(bridge, project) })
          }
          onWorktreeMenu={(project, worktree, anchor) =>
            openMenu({ ...anchor, items: worktreeMenuItems(bridge, project, worktree) })
          }
          onArchiveSession={(row) => void archiveSession(bridge, row)}
          onRestoreSession={(id) => void useSessionStore.getState().restore(bridge, id)}
        />

        <SessionSurface active={view === "session"} />

        <ContextPanel
          active={view === "context"}
          session={detail}
          project={activeProject}
        />

        {!contextOpen ? (
          <div className="ctx-stub">
            <button
              className="icon-btn"
              type="button"
              aria-controls="panel-context"
              aria-expanded={false}
              aria-label="Show context panel"
              title="Show context panel"
              onClick={() => setContextOpen(true)}
            >
              <Icon name="chevronLeft" />
            </button>
          </div>
        ) : null}
      </div>

      <StatusBar
        info={info}
        sessions={sessions}
        session={detail}
        project={activeProject}
        branch={git?.branch ?? activeWorktree?.branch ?? null}
        events={events}
        plan={planStats}
        prefs={prefs}
      />

      <NewSessionDialog />
      <AddProjectDialog />
      <DiffModal />
      <PrDialog project={activeProject} />
      <SettingsModal />
      <ContextMenuHost />
      <Toast />
    </div>
  );
}
