import type { GitWorktree, Project, SessionSummary, SubagentSummary } from "@grove/contract";
import { SESSION_TITLE_MAX_CHARS } from "@grove/contract";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from "react";
import { useBridge } from "../ipc/context";
import { useSessionStore } from "../state/sessionStore";
import type { MenuAnchor } from "../state/menuStore";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore } from "../state/workspaceStore";
import { providerMeta, sessionVisual } from "../workspace/models";
import { Icon } from "../workspace/Icon";
import { basename, normalizePath, projectForSession, worktreeForSession } from "../workspace/tree";

interface NavTreeProps {
  active: boolean;
  filter: string;
  onFilter: (value: string) => void;
  activeId: string | null;
  onSelect: (id: string) => void;
  onNewSession: (projectId: string | null, worktreePath: string | null) => void;
  onNewChat: () => void;
  onAddProject: () => void;
  onSettings: () => void;
  /** Surfaces open the app-level menu; the shell owns the items and their actions. */
  onSessionMenu?: (session: SessionSummary, anchor: MenuAnchor) => void;
  onProjectMenu?: (project: Project, anchor: MenuAnchor) => void;
  onWorktreeMenu?: (project: Project, worktree: GitWorktree, anchor: MenuAnchor) => void;
  /** The row-level hover actions; the shell owns the bridge and the confirmation. */
  onArchiveSession?: (session: SessionSummary) => void;
  onRestoreSession?: (id: string) => void;
}

function anchorOf(event: ReactMouseEvent<HTMLElement>): MenuAnchor {
  return { x: event.clientX, y: event.clientY, returnFocus: event.currentTarget };
}

function matches(session: SessionSummary, project: Project | null, query: string): boolean {
  if (!query) return true;
  const haystack = [
    session.title,
    session.provider,
    project?.name ?? "chats",
    session.cwd,
    ...(session.subagents ?? []).map((subagent) => subagent.description),
  ]
    .join(" ")
    .toLowerCase();
  return haystack.includes(query);
}

function subagentVisual(status: SubagentSummary["status"]): string {
  switch (status) {
    case "running":
      return "running";
    case "ok":
      return "done";
    case "error":
      return "failed";
    case "cancelled":
      return "idle";
  }
}

function SubagentRow({
  subagent,
  sessionId,
  onSelect,
}: {
  subagent: SubagentSummary;
  sessionId: string;
  onSelect: (id: string) => void;
}) {
  const openSubagent = useUiStore((state) => state.openSubagent);
  const visual = subagentVisual(subagent.status);
  return (
    <button
      className="subagent-row"
      type="button"
      data-state={visual}
      onClick={() => {
        openSubagent(sessionId, subagent.callId);
        onSelect(sessionId);
      }}
      title={subagent.summary ?? subagent.description}
    >
      <span className="dot" data-state={visual} aria-hidden="true" />
      <Icon name="sparkle" size="xs" />
      <span className="s-title">{subagent.description || "subagent"}</span>
      <span className="sr-only">{subagent.status}</span>
    </button>
  );
}

/**
 * The tree row while it is being renamed: an input in place of the row button, so a click cannot
 * open the session underneath. Enter and blur commit, Escape and an empty value cancel.
 */
function SessionTitleEditor({ session, active }: { session: SessionSummary; active: boolean }) {
  const bridge = useBridge();
  const rename = useSessionStore((state) => state.rename);
  const endRename = useUiStore((state) => state.endRename);
  const field = useRef<HTMLInputElement>(null);
  /** Set once the edit has settled, so the unmount blur cannot commit a cancelled edit. */
  const settled = useRef(false);
  const [value, setValue] = useState(session.title);
  const [error, setError] = useState<string | null>(null);
  const visual = sessionVisual(session);

  useEffect(() => {
    field.current?.focus();
    field.current?.select();
  }, []);

  const close = () => {
    settled.current = true;
    endRename();
  };

  const commit = async () => {
    if (settled.current) return;
    const title = value.trim();
    if (!title) {
      close();
      return;
    }
    if (title.length > SESSION_TITLE_MAX_CHARS) {
      setError(`Titles are capped at ${SESSION_TITLE_MAX_CHARS} characters.`);
      return;
    }
    settled.current = true;
    const failure = await rename(bridge, session.id, title);
    if (failure) {
      settled.current = false;
      setError(failure);
      return;
    }
    endRename();
  };

  return (
    <>
      <div
        className="session-row is-editing"
        data-state={visual}
        aria-current={active ? "true" : undefined}
      >
        <span className="dot" data-state={visual} aria-hidden="true" />
        <input
          ref={field}
          className="session-rename"
          type="text"
          aria-label="Session title"
          aria-invalid={error ? true : undefined}
          autoComplete="off"
          spellCheck={false}
          value={value}
          onChange={(event) => {
            setValue(event.target.value);
            setError(null);
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              event.stopPropagation();
              void commit();
            } else if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              close();
            }
          }}
          onBlur={() => void commit()}
        />
      </div>
      {error ? (
        <p className="session-rename-error" role="alert">
          {error}
        </p>
      ) : null}
    </>
  );
}

function SessionRow({
  session,
  active,
  archived,
  suffix,
  onSelect,
  onMenu,
  onArchive,
  onRestore,
}: {
  session: SessionSummary;
  active: boolean;
  archived?: boolean;
  /** A quiet trailing label on archived rows: the project the session came from. */
  suffix?: string | null;
  onSelect: (id: string) => void;
  onMenu?: (session: SessionSummary, anchor: MenuAnchor) => void;
  onArchive?: (session: SessionSummary) => void;
  onRestore?: (id: string) => void;
}) {
  const closeSubagent = useUiStore((state) => state.closeSubagent);
  const editing = useUiStore((state) => state.renamingSession === session.id);
  const visual = sessionVisual(session);
  const subagents = session.subagents ?? [];
  return (
    <div className="session-block" data-archived={archived ? "true" : undefined}>
      {editing ? (
        <SessionTitleEditor session={session} active={active} />
      ) : (
        <button
          className="session-row"
          type="button"
          data-state={visual}
          data-archived={archived ? "true" : undefined}
          aria-current={active ? "true" : undefined}
          onClick={() => {
            closeSubagent();
            onSelect(session.id);
          }}
          onContextMenu={(event) => {
            if (!onMenu) return;
            event.preventDefault();
            onMenu(session, anchorOf(event));
          }}
          title={session.title}
        >
          <span className="dot" data-state={visual} aria-hidden="true" />
          <span className="s-title">{session.title}</span>
          {session.pinned ? (
            <span className="s-pin" title="Pinned">
              <Icon name="pin" size="xs" />
            </span>
          ) : null}
          {suffix ? <span className="s-suffix">{suffix}</span> : null}
          {subagents.length ? (
            <span
              className="subagent-count"
              title={`${subagents.length} subagent${subagents.length === 1 ? "" : "s"}`}
            >
              {subagents.length}
            </span>
          ) : null}
          <span className="sr-only">{visual}</span>
        </button>
      )}
      {!editing ? (
        <div className="session-actions">
          {archived && onRestore ? (
            <button
              className="icon-btn sm"
              type="button"
              aria-label="Restore session"
              title="Restore"
              onClick={(event) => {
                event.stopPropagation();
                onRestore(session.id);
              }}
            >
              <Icon name="restore" size="sm" />
            </button>
          ) : null}
          {!archived && onArchive ? (
            <button
              className="icon-btn sm"
              type="button"
              aria-label="Archive session"
              title="Archive"
              onClick={(event) => {
                event.stopPropagation();
                onArchive(session);
              }}
            >
              <Icon name="trash" size="sm" />
            </button>
          ) : null}
          {onMenu ? (
            <button
              className="icon-btn sm"
              type="button"
              aria-label="Session actions"
              title="Session actions"
              onClick={(event) => {
                event.stopPropagation();
                const box = event.currentTarget.getBoundingClientRect();
                onMenu(session, {
                  x: box.left,
                  y: box.bottom + 2,
                  returnFocus: event.currentTarget,
                });
              }}
              onContextMenu={(event) => {
                event.preventDefault();
                onMenu(session, anchorOf(event));
              }}
            >
              <Icon name="more" size="sm" />
            </button>
          ) : null}
        </div>
      ) : null}
      {subagents.length ? (
        <div className="subagent-list">
          {subagents.map((subagent) => (
            <SubagentRow
              key={subagent.callId}
              subagent={subagent}
              sessionId={session.id}
              onSelect={onSelect}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

function Caret({
  nodeId,
  label,
  expanded,
  onToggle,
}: {
  nodeId: string;
  label: string;
  expanded: boolean;
  onToggle: (id: string) => void;
}) {
  return (
    <button
      className="caret"
      type="button"
      aria-expanded={expanded}
      aria-label={label}
      onClick={() => onToggle(nodeId)}
    >
      <Icon name="chevron" size="sm" />
    </button>
  );
}

const NAV_VIEW_KEY = "grove.navView";

type NavSort = "recent" | "created" | "title" | "project" | "status";
type NavDirection = "asc" | "desc";
type NavRange = "any" | "today" | "week";

interface NavFilters {
  needsYou: boolean;
  running: boolean;
  failed: boolean;
  providers: string[];
  /** Project ids, plus `__chats` for sessions with no project. */
  projects: string[];
  range: NavRange;
}

interface NavView {
  archive: boolean;
  flat: boolean;
  sort: NavSort;
  direction: NavDirection;
  filters: NavFilters;
}

const SORT_LABEL: Record<NavSort, string> = {
  recent: "Recent activity",
  created: "Created",
  title: "Title",
  project: "Project",
  status: "Status",
};

const SORT_DEFAULT: Record<NavSort, NavDirection> = {
  recent: "desc",
  created: "desc",
  title: "asc",
  project: "asc",
  status: "asc",
};

function emptyFilters(): NavFilters {
  return {
    needsYou: false,
    running: false,
    failed: false,
    providers: [],
    projects: [],
    range: "any",
  };
}

function readNavView(): NavView {
  const fallback: NavView = {
    archive: false,
    flat: false,
    sort: "recent",
    direction: "desc",
    filters: emptyFilters(),
  };
  try {
    const raw = localStorage.getItem(NAV_VIEW_KEY);
    if (!raw) return fallback;
    const value = JSON.parse(raw) as Partial<NavView>;
    const sort =
      value.sort && Object.hasOwn(SORT_LABEL, value.sort) ? value.sort : fallback.sort;
    const filters = value.filters ?? emptyFilters();
    return {
      archive: value.archive === true,
      flat: value.flat === true,
      sort,
      direction: value.direction === "asc" ? "asc" : "desc",
      filters: {
        needsYou: filters.needsYou === true,
        running: filters.running === true,
        failed: filters.failed === true,
        providers: Array.isArray(filters.providers)
          ? filters.providers.filter((id): id is string => typeof id === "string")
          : [],
        projects: Array.isArray(filters.projects)
          ? filters.projects.filter((id): id is string => typeof id === "string")
          : [],
        range:
          filters.range === "today" || filters.range === "week" ? filters.range : "any",
      },
    };
  } catch {
    return fallback;
  }
}

function countFilters(filters: NavFilters): number {
  return (
    (filters.needsYou ? 1 : 0) +
    (filters.running ? 1 : 0) +
    (filters.failed ? 1 : 0) +
    filters.providers.length +
    filters.projects.length +
    (filters.range === "any" ? 0 : 1)
  );
}

function statusRank(session: SessionSummary): number {
  switch (sessionVisual(session)) {
    case "waiting":
      return 0;
    case "running":
      return 1;
    case "failed":
      return 2;
    case "idle":
      return 3;
    case "done":
      return 4;
  }
}

export function NavTree({
  active,
  filter,
  onFilter,
  activeId,
  onSelect,
  onNewSession,
  onNewChat,
  onAddProject,
  onSettings,
  onSessionMenu,
  onProjectMenu,
  onWorktreeMenu,
  onArchiveSession,
  onRestoreSession,
}: NavTreeProps) {
  const sessions = useSessionStore((state) => state.sessions);
  const projects = useWorkspaceStore((state) => state.projects);
  const worktrees = useWorkspaceStore((state) => state.worktrees);
  const [view, setView] = useState<NavView>(readNavView);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [filterOpen, setFilterOpen] = useState(false);
  const [sortOpen, setSortOpen] = useState(false);
  const topRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    try {
      localStorage.setItem(NAV_VIEW_KEY, JSON.stringify(view));
    } catch {
      // A blocked store must not stop the sidebar from changing.
    }
  }, [view]);

  useEffect(() => {
    if (!filterOpen && !sortOpen) return;
    const close = (event: PointerEvent) => {
      if (topRef.current && !topRef.current.contains(event.target as Node)) {
        setFilterOpen(false);
        setSortOpen(false);
      }
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setFilterOpen(false);
        setSortOpen(false);
      }
    };
    window.addEventListener("pointerdown", close);
    window.addEventListener("keydown", key);
    return () => {
      window.removeEventListener("pointerdown", close);
      window.removeEventListener("keydown", key);
    };
  }, [filterOpen, sortOpen]);

  const activeSessions = useMemo(
    () => sessions.filter((session) => !session.archivedAt),
    [sessions],
  );
  const archivedSessions = useMemo(
    () => sessions.filter((session) => session.archivedAt),
    [sessions],
  );

  const query = filter.trim().toLowerCase();
  const filters = view.filters;
  const filterCount = countFilters(filters);
  const awaiting = activeSessions.filter((session) => session.awaiting).length;

  const today = useMemo(() => {
    const date = new Date();
    date.setHours(0, 0, 0, 0);
    return date.getTime();
  }, []);

  const passes = (session: SessionSummary): boolean => {
    const hasStatus = filters.needsYou || filters.running || filters.failed;
    if (hasStatus) {
      const visual = sessionVisual(session);
      const statusMatch =
        (filters.needsYou && session.awaiting) ||
        (filters.running && visual === "running") ||
        (filters.failed && visual === "failed");
      if (!statusMatch) return false;
    }
    if (filters.providers.length && !filters.providers.includes(session.provider)) {
      return false;
    }
    if (filters.projects.length) {
      const projectId = projectForSession(session, projects)?.id ?? "__chats";
      if (!filters.projects.includes(projectId)) return false;
    }
    if (filters.range !== "any") {
      const updated = Date.parse(session.updatedAt);
      const threshold = filters.range === "today" ? today : Date.now() - 7 * 24 * 60 * 60 * 1000;
      if (!Number.isFinite(updated) || updated < threshold) return false;
    }
    return true;
  };

  const compare = (left: SessionSummary, right: SessionSummary): number => {
    const projectName = (session: SessionSummary) =>
      projectForSession(session, projects)?.name ?? "";
    let result = 0;
    switch (view.sort) {
      case "recent":
        result = Date.parse(left.updatedAt) - Date.parse(right.updatedAt);
        break;
      case "created":
        result = Date.parse(left.createdAt) - Date.parse(right.createdAt);
        break;
      case "title":
        result = left.title.localeCompare(right.title, undefined, { sensitivity: "base" });
        break;
      case "project":
        result =
          projectName(left).localeCompare(projectName(right), undefined, { sensitivity: "base" }) ||
          Date.parse(right.updatedAt) - Date.parse(left.updatedAt);
        break;
      case "status":
        result =
          statusRank(left) - statusRank(right) ||
          Date.parse(right.updatedAt) - Date.parse(left.updatedAt);
        break;
    }
    return view.direction === "asc" ? result : -result;
  };

  const sortSessions = (list: SessionSummary[]) => [...list].sort(compare);

  const source = view.archive ? archivedSessions : activeSessions;
  const shown = source.filter(
    (session) => matches(session, projectForSession(session, projects), query) && passes(session),
  );

  const chats = sortSessions(shown.filter((session) => !projectForSession(session, projects)));
  const pinned =
    view.archive || view.flat ? [] : sortSessions(shown.filter((session) => session.pinned));
  const recent =
    view.archive || view.flat || activeSessions.length <= 5
      ? []
      : [...activeSessions]
          .filter(
            (session) =>
              matches(session, projectForSession(session, projects), query) && passes(session),
          )
          .sort((left, right) => Date.parse(right.updatedAt) - Date.parse(left.updatedAt))
          .slice(0, 5);
  const flatRows = !view.archive && view.flat ? sortSessions(shown) : [];
  const orderedProjects =
    view.sort === "project"
      ? [...projects].sort(
          (left, right) =>
            left.name.localeCompare(right.name, undefined, { sensitivity: "base" }) *
            (view.direction === "asc" ? 1 : -1),
        )
      : projects;

  const toggle = (id: string) => {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };
  const isOpen = (id: string) => query.length > 0 || !collapsed.has(id);

  const worktreeIds = projects.flatMap((project) =>
    (worktrees[project.id] ?? []).map((worktree) => `w-${project.id}-${worktree.path}`),
  );
  const archiveGroupIds = (() => {
    const ids = new Set<string>();
    if (shown.some((session) => !projectForSession(session, projects))) ids.add("a-chats");
    for (const session of shown) {
      const project = projectForSession(session, projects);
      if (project) ids.add(`a-${project.id}`);
    }
    return [...ids];
  })();
  const groupIds = view.archive
    ? archiveGroupIds
    : view.flat
      ? []
      : ["chats", "pinned", "recent", ...projects.map((project) => `p-${project.id}`), ...worktreeIds];
  const allCollapsed = groupIds.length > 0 && groupIds.every((id) => collapsed.has(id));
  const toggleAll = () => setCollapsed(allCollapsed ? new Set() : new Set(groupIds));

  const summary = view.archive
    ? `${archivedSessions.length} archived`
    : `${activeSessions.length} session${activeSessions.length === 1 ? "" : "s"} · ${awaiting} awaiting you`;

  const providerIds = [...new Set(sessions.map((session) => session.provider))].sort();
  const projectOptions = [
    { id: "__chats", name: "Chats" },
    ...projects.map((project) => ({ id: project.id, name: project.name })),
  ];

  const updateFilters = (patch: Partial<NavFilters>) =>
    setView((current) => ({ ...current, filters: { ...current.filters, ...patch } }));
  const toggleFilterValue = (key: "providers" | "projects", value: string) =>
    setView((current) => {
      const list = current.filters[key];
      return {
        ...current,
        filters: {
          ...current.filters,
          [key]: list.includes(value) ? list.filter((entry) => entry !== value) : [...list, value],
        },
      };
    });

  const renderProject = (project: Project) => {
    const all = activeSessions.filter(
      (session) => projectForSession(session, projects)?.id === project.id,
    );
    const visible = sortSessions(
      shown.filter((session) => projectForSession(session, projects)?.id === project.id),
    );
    const projectMatches = !query || project.name.toLowerCase().includes(query);
    if ((query || filterCount) && visible.length === 0 && !projectMatches) return null;
    const open = isOpen(`p-${project.id}`);
    const projectAwaiting = all.filter((session) => session.awaiting).length;
    const worktreeList = project.isGit
      ? (worktrees[project.id] ?? []).length > 0
        ? worktrees[project.id]
        : [
            {
              path: project.path,
              head: null,
              branch: project.defaultBranch,
              bare: false,
              detached: false,
              current: true,
            },
          ]
      : [];
    const loose = project.isGit
      ? visible.filter((session) => !worktreeForSession(session, worktreeList))
      : visible;

    return (
      <div className="tree-group" key={project.id}>
        <div className="row-line">
          <Caret
            nodeId={`p-${project.id}`}
            label={`Toggle ${project.name}`}
            expanded={open}
            onToggle={toggle}
          />
          <button
            className="node-row is-root"
            type="button"
            aria-expanded={open}
            aria-controls={`kids-p-${project.id}`}
            onClick={() => toggle(`p-${project.id}`)}
            onContextMenu={(event) => {
              if (!onProjectMenu) return;
              event.preventDefault();
              onProjectMenu(project, anchorOf(event));
            }}
          >
            <span className="node-ico">
              <Icon name="folder" size="sm" />
            </span>
            <span className="node-name">{project.name}</span>
            {!project.isGit ? <span className="node-tag">no git</span> : null}
            {projectAwaiting ? (
              <span className="node-await" title={`${projectAwaiting} awaiting you`}>
                {projectAwaiting}
              </span>
            ) : null}
            <span className="node-count">{all.length}</span>
          </button>
          <button
            className="node-add"
            type="button"
            aria-label={`New session in ${project.name}`}
            title="New session"
            onClick={() => onNewSession(project.id, null)}
          >
            <Icon name="plus" size="sm" />
          </button>
        </div>
        <div className="kids" id={`kids-p-${project.id}`} hidden={!open}>
          {project.isGit
            ? worktreeList.map((worktree) => {
                const inWorktree = sortSessions(
                  visible.filter((session) => worktreeForSession(session, [worktree]) !== null),
                );
                const allInWorktree = all.filter(
                  (session) => worktreeForSession(session, [worktree]) !== null,
                );
                const worktreeId = `w-${project.id}-${worktree.path}`;
                const worktreeOpen = isOpen(worktreeId);
                return (
                  <div className="tree-group" key={worktreeId}>
                    <div className="row-line">
                      <Caret
                        nodeId={worktreeId}
                        label={`Toggle ${worktree.branch ?? basename(worktree.path)}`}
                        expanded={worktreeOpen}
                        onToggle={toggle}
                      />
                      <button
                        className="node-row"
                        type="button"
                        aria-expanded={worktreeOpen}
                        aria-controls={`kids-${worktreeId}`}
                        onClick={() => toggle(worktreeId)}
                        onContextMenu={(event) => {
                          if (!onWorktreeMenu) return;
                          event.preventDefault();
                          onWorktreeMenu(project, worktree, anchorOf(event));
                        }}
                      >
                        <span className="node-ico">
                          <Icon name="branch" size="sm" />
                        </span>
                        <span className="node-name">{worktree.branch ?? basename(worktree.path)}</span>
                        {normalizePath(worktree.path) === normalizePath(project.path) ? (
                          <span className="node-tag">main</span>
                        ) : null}
                        <span className="node-count">{allInWorktree.length}</span>
                      </button>
                      <button
                        className="node-add"
                        type="button"
                        data-new-wt={worktree.path}
                        data-project={project.id}
                        aria-label={`New session on ${worktree.branch ?? worktree.path}`}
                        onClick={() => onNewSession(project.id, worktree.path)}
                      >
                        <Icon name="plus" size="sm" />
                      </button>
                    </div>
                    <div className="kids" id={`kids-${worktreeId}`} hidden={!worktreeOpen}>
                      {inWorktree.length ? (
                        inWorktree.map((session) => (
                          <SessionRow
                            key={session.id}
                            session={session}
                            active={session.id === activeId}
                            onSelect={onSelect}
                            onMenu={onSessionMenu}
                            onArchive={onArchiveSession}
                            onRestore={onRestoreSession}
                          />
                        ))
                      ) : (
                        <p className="tree-empty">No sessions yet</p>
                      )}
                    </div>
                  </div>
                );
              })
            : null}
          {loose.length
            ? loose.map((session) => (
                <SessionRow
                  key={session.id}
                  session={session}
                  active={session.id === activeId}
                  onSelect={onSelect}
                  onMenu={onSessionMenu}
                  onArchive={onArchiveSession}
                  onRestore={onRestoreSession}
                />
              ))
            : project.isGit
              ? null
              : <p className="tree-empty">No sessions yet</p>}
        </div>
      </div>
    );
  };

  const archiveProjects = orderedProjects
    .map((project) => ({
      project,
      rows: sortSessions(
        shown.filter((session) => projectForSession(session, projects)?.id === project.id),
      ),
    }))
    .filter((group) => group.rows.length > 0);

  const archiveChats = sortSessions(
    shown.filter((session) => !projectForSession(session, projects)),
  );

  const noResults =
    shown.length === 0 && (query.length > 0 || filterCount > 0 || view.archive || view.flat);
  const archiveGroupsOpen = isOpen("a-chats");

  return (
    <aside
      className={active ? "panel nav is-active" : "panel nav"}
      id="panel-nav"
      aria-label="Projects, worktrees and sessions"
    >
      <div className="nav-top" ref={topRef}>
        <div className="nav-head">
          <div className="nav-title">
            <strong>{view.archive ? "Archive" : "Sessions"}</strong>
            <span className="nav-count">
              {view.archive ? archivedSessions.length : activeSessions.length}
            </span>
            {!view.archive && awaiting ? (
              <span className="nav-await" title={`${awaiting} awaiting you`}>
                {awaiting}
              </span>
            ) : null}
          </div>
          <div className="nav-actions">
            <button
              className="nav-btn"
              type="button"
              aria-label="New chat"
              title="New chat"
              onClick={onNewChat}
            >
              <Icon name="plus" size="sm" />
            </button>
            <button
              className="nav-btn"
              type="button"
              aria-label="Add project"
              title="Add project"
              onClick={onAddProject}
            >
              <Icon name="folderPlus" size="sm" />
            </button>
            <button
              className="nav-btn"
              type="button"
              aria-label={view.archive ? "Show active sessions" : "Show archived sessions"}
              title={view.archive ? "Active sessions" : "Archived sessions"}
              aria-pressed={view.archive}
              onClick={() => setView((current) => ({ ...current, archive: !current.archive }))}
            >
              <Icon name="archive" size="sm" />
              {archivedSessions.length ? (
                <span className="nav-badge">{archivedSessions.length}</span>
              ) : null}
            </button>
            <button
              className="nav-btn"
              type="button"
              aria-label="Filter sessions"
              title="Filter sessions"
              aria-pressed={filterOpen}
              onClick={() => {
                setSortOpen(false);
                setFilterOpen((open) => !open);
              }}
            >
              <Icon name="filter" size="sm" />
              {filterCount ? <span className="nav-badge">{filterCount}</span> : null}
            </button>
            <button
              className="nav-btn"
              type="button"
              aria-label={`Sort by ${SORT_LABEL[view.sort]}`}
              title={`Sort: ${SORT_LABEL[view.sort]}`}
              aria-pressed={sortOpen}
              onClick={() => {
                setFilterOpen(false);
                setSortOpen((open) => !open);
              }}
            >
              <Icon name="sort" size="sm" />
            </button>
            <button
              className="nav-btn"
              type="button"
              aria-label={allCollapsed ? "Expand all" : "Collapse all"}
              title={allCollapsed ? "Expand all" : "Collapse all"}
              disabled={groupIds.length === 0}
              onClick={toggleAll}
            >
              <Icon name="collapseAll" size="sm" />
            </button>
            {!view.archive ? (
              <button
                className="nav-btn"
                type="button"
                aria-label={view.flat ? "Show the project tree" : "Show a flat session list"}
                title={view.flat ? "Tree view" : "Flat list"}
                aria-pressed={view.flat}
                onClick={() => setView((current) => ({ ...current, flat: !current.flat }))}
              >
                <Icon name="list" size="sm" />
              </button>
            ) : null}
          </div>
        </div>

        <div className="search">
          <span className="search-ico">
            <Icon name="search" size="sm" />
          </span>
          <label className="sr-only" htmlFor="nav-filter">
            Filter projects, branches and sessions
          </label>
          <input
            id="nav-filter"
            type="search"
            placeholder="Filter sessions"
            autoComplete="off"
            value={filter}
            onChange={(event) => onFilter(event.target.value)}
          />
        </div>

        {filterOpen ? (
          <div className="nav-pop" role="group" aria-label="Session filters">
            <h4>Status</h4>
            <label>
              <input
                type="checkbox"
                checked={filters.needsYou}
                onChange={(event) => updateFilters({ needsYou: event.target.checked })}
              />
              Needs you
            </label>
            <label>
              <input
                type="checkbox"
                checked={filters.running}
                onChange={(event) => updateFilters({ running: event.target.checked })}
              />
              Running
            </label>
            <label>
              <input
                type="checkbox"
                checked={filters.failed}
                onChange={(event) => updateFilters({ failed: event.target.checked })}
              />
              Failed
            </label>

            {providerIds.length ? <h4>Provider</h4> : null}
            {providerIds.map((id) => (
              <label key={id}>
                <input
                  type="checkbox"
                  checked={filters.providers.includes(id)}
                  onChange={() => toggleFilterValue("providers", id)}
                />
                {providerMeta(id).name}
              </label>
            ))}

            <h4>Project</h4>
            {projectOptions.map((project) => (
              <label key={project.id}>
                <input
                  type="checkbox"
                  checked={filters.projects.includes(project.id)}
                  onChange={() => toggleFilterValue("projects", project.id)}
                />
                {project.name}
              </label>
            ))}

            <h4>Updated</h4>
            <select
              className="nav-select"
              value={filters.range}
              onChange={(event) =>
                updateFilters({ range: event.target.value as NavRange })
              }
            >
              <option value="any">Any time</option>
              <option value="today">Today</option>
              <option value="week">Last 7 days</option>
            </select>

            <div className="nav-pop-foot">
              <button
                className="btn-ghost sm"
                type="button"
                onClick={() => updateFilters(emptyFilters())}
              >
                Clear all
              </button>
              <button className="btn-ghost sm" type="button" onClick={() => setFilterOpen(false)}>
                Done
              </button>
            </div>
          </div>
        ) : null}

        {sortOpen ? (
          <div className="nav-pop nav-sort" role="group" aria-label="Sort sessions">
            <h4>Sort by</h4>
            {(Object.keys(SORT_LABEL) as NavSort[]).map((key) => (
              <button
                key={key}
                className="nav-sort-row"
                type="button"
                aria-pressed={view.sort === key}
                onClick={() =>
                  setView((current) => ({
                    ...current,
                    sort: key,
                    direction: SORT_DEFAULT[key],
                  }))
                }
              >
                <span>{SORT_LABEL[key]}</span>
                {view.sort === key ? <Icon name="check" size="sm" /> : null}
              </button>
            ))}
            <h4>Direction</h4>
            <div className="nav-dir">
              <button
                type="button"
                aria-pressed={view.direction === "asc"}
                onClick={() => setView((current) => ({ ...current, direction: "asc" }))}
              >
                Ascending
              </button>
              <button
                type="button"
                aria-pressed={view.direction === "desc"}
                onClick={() => setView((current) => ({ ...current, direction: "desc" }))}
              >
                Descending
              </button>
            </div>
          </div>
        ) : null}
      </div>

      <div className="nav-scroll od-scroll">
        <div className="tree">
          {view.archive ? (
            <>
              {archiveChats.length ? (
                <div className="tree-group">
                  <div className="row-line">
                    <Caret
                      nodeId="a-chats"
                      label="Toggle archived chats"
                      expanded={archiveGroupsOpen}
                      onToggle={toggle}
                    />
                    <button
                      className="node-row is-root"
                      type="button"
                      aria-expanded={archiveGroupsOpen}
                      aria-controls="kids-a-chats"
                      onClick={() => toggle("a-chats")}
                    >
                      <span className="node-ico">
                        <Icon name="message" size="sm" />
                      </span>
                      <span className="node-name">Chats</span>
                      <span className="node-count">{archiveChats.length}</span>
                    </button>
                  </div>
                  <div className="kids" id="kids-a-chats" hidden={!archiveGroupsOpen}>
                    {archiveChats.map((session) => (
                      <SessionRow
                        key={session.id}
                        session={session}
                        active={session.id === activeId}
                        archived
                        onSelect={onSelect}
                        onMenu={onSessionMenu}
                        onArchive={onArchiveSession}
                        onRestore={onRestoreSession}
                      />
                    ))}
                  </div>
                </div>
              ) : null}
              {archiveProjects.map(({ project, rows }) => {
                const id = `a-${project.id}`;
                const open = isOpen(id);
                return (
                  <div className="tree-group" key={id}>
                    <div className="row-line">
                      <Caret
                        nodeId={id}
                        label={`Toggle archived ${project.name}`}
                        expanded={open}
                        onToggle={toggle}
                      />
                      <button
                        className="node-row is-root"
                        type="button"
                        aria-expanded={open}
                        aria-controls={`kids-${id}`}
                        onClick={() => toggle(id)}
                      >
                        <span className="node-ico">
                          <Icon name="folder" size="sm" />
                        </span>
                        <span className="node-name">{project.name}</span>
                        <span className="node-count">{rows.length}</span>
                      </button>
                    </div>
                    <div className="kids" id={`kids-${id}`} hidden={!open}>
                      {rows.map((session) => (
                        <SessionRow
                          key={session.id}
                          session={session}
                          active={session.id === activeId}
                          archived
                          onSelect={onSelect}
                          onMenu={onSessionMenu}
                          onArchive={onArchiveSession}
                          onRestore={onRestoreSession}
                        />
                      ))}
                    </div>
                  </div>
                );
              })}
            </>
          ) : view.flat ? (
            flatRows.map((session) => (
              <SessionRow
                key={session.id}
                session={session}
                active={session.id === activeId}
                suffix={projectForSession(session, projects)?.name ?? "Chat"}
                onSelect={onSelect}
                onMenu={onSessionMenu}
                onArchive={onArchiveSession}
                onRestore={onRestoreSession}
              />
            ))
          ) : (
            <>
              {pinned.length ? (
                <div className="tree-group">
                  <div className="row-line">
                    <Caret
                      nodeId="pinned"
                      label="Toggle pinned sessions"
                      expanded={isOpen("pinned")}
                      onToggle={toggle}
                    />
                    <button
                      className="node-row is-root"
                      type="button"
                      aria-expanded={isOpen("pinned")}
                      aria-controls="kids-pinned"
                      onClick={() => toggle("pinned")}
                    >
                      <span className="node-ico">
                        <Icon name="pin" size="sm" />
                      </span>
                      <span className="node-name">Pinned</span>
                      <span className="node-count">{pinned.length}</span>
                    </button>
                  </div>
                  <div className="kids" id="kids-pinned" hidden={!isOpen("pinned")}>
                    {pinned.map((session) => (
                      <SessionRow
                        key={session.id}
                        session={session}
                        active={session.id === activeId}
                        onSelect={onSelect}
                        onMenu={onSessionMenu}
                        onArchive={onArchiveSession}
                        onRestore={onRestoreSession}
                      />
                    ))}
                  </div>
                </div>
              ) : null}

              {recent.length ? (
                <div className="tree-group">
                  <div className="row-line">
                    <Caret
                      nodeId="recent"
                      label="Toggle recent sessions"
                      expanded={isOpen("recent")}
                      onToggle={toggle}
                    />
                    <button
                      className="node-row is-root"
                      type="button"
                      aria-expanded={isOpen("recent")}
                      aria-controls="kids-recent"
                      onClick={() => toggle("recent")}
                    >
                      <span className="node-ico">
                        <Icon name="clock" size="sm" />
                      </span>
                      <span className="node-name">Recent</span>
                      <span className="node-count">{recent.length}</span>
                    </button>
                  </div>
                  <div className="kids" id="kids-recent" hidden={!isOpen("recent")}>
                    {recent.map((session) => (
                      <SessionRow
                        key={session.id}
                        session={session}
                        active={session.id === activeId}
                        onSelect={onSelect}
                        onMenu={onSessionMenu}
                        onArchive={onArchiveSession}
                        onRestore={onRestoreSession}
                      />
                    ))}
                  </div>
                </div>
              ) : null}

              <div className="tree-group">
                <div className="row-line">
                  <Caret
                    nodeId="chats"
                    label="Toggle chats"
                    expanded={isOpen("chats")}
                    onToggle={toggle}
                  />
                  <button
                    className="node-row is-root"
                    type="button"
                    aria-expanded={isOpen("chats")}
                    aria-controls="kids-chats"
                    onClick={() => toggle("chats")}
                  >
                    <span className="node-ico">
                      <Icon name="message" size="sm" />
                    </span>
                    <span className="node-name">Chats</span>
                    <span className="node-count">{chats.length}</span>
                  </button>
                  <button
                    className="node-add"
                    type="button"
                    aria-label="New chat in Chats"
                    onClick={onNewChat}
                  >
                    <Icon name="plus" size="sm" />
                  </button>
                </div>
                <div className="kids" id="kids-chats" hidden={!isOpen("chats")}>
                  {chats.length ? (
                    chats.map((session) => (
                      <SessionRow
                        key={session.id}
                        session={session}
                        active={session.id === activeId}
                        onSelect={onSelect}
                        onMenu={onSessionMenu}
                        onArchive={onArchiveSession}
                        onRestore={onRestoreSession}
                      />
                    ))
                  ) : (
                    <p className="tree-empty">No chats yet</p>
                  )}
                </div>
              </div>

              {orderedProjects.map(renderProject)}
            </>
          )}

          {noResults ? (
            <p className="tree-empty" style={{ paddingLeft: 4 }}>
              {query
                ? `Nothing matches “${filter}”.`
                : filterCount
                  ? "No sessions match these filters."
                  : view.archive
                    ? "No archived sessions."
                    : "No sessions yet."}
            </p>
          ) : null}
        </div>
      </div>

      <div className="nav-foot">
        <span>{summary}</span>
        <button
          className="icon-btn sm"
          type="button"
          aria-label="Settings"
          title="Settings"
          onClick={onSettings}
        >
          <Icon name="settings" size="sm" />
        </button>
      </div>
    </aside>
  );
}
