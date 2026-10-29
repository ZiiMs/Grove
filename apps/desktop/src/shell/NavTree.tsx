import type { GitWorktree, Project, SessionSummary, SubagentSummary } from "@grove/contract";
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
}

function anchorOf(event: ReactMouseEvent<HTMLElement>): MenuAnchor {
  return { x: event.clientX, y: event.clientY, returnFocus: event.currentTarget };
}

function ProviderMark({ provider, small }: { provider: string; small?: boolean }) {
  const meta = providerMeta(provider);
  return (
    <span
      className={small ? "mark sm" : "mark"}
      style={{ ["--tint" as string]: meta.tint }}
      aria-hidden="true"
    >
      {meta.short}
    </span>
  );
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

/** The core refuses titles longer than this; the field checks first so the message lands inline. */
const MAX_SESSION_TITLE_CHARS = 80;

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
    if (title.length > MAX_SESSION_TITLE_CHARS) {
      setError(`Titles are capped at ${MAX_SESSION_TITLE_CHARS} characters.`);
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
        <ProviderMark provider={session.provider} small />
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
  onSelect,
  onMenu,
}: {
  session: SessionSummary;
  active: boolean;
  onSelect: (id: string) => void;
  onMenu?: (session: SessionSummary, anchor: MenuAnchor) => void;
}) {
  const closeSubagent = useUiStore((state) => state.closeSubagent);
  const editing = useUiStore((state) => state.renamingSession === session.id);
  const visual = sessionVisual(session);
  const subagents = session.subagents ?? [];
  return (
    <div className="session-block">
      {editing ? (
        <SessionTitleEditor session={session} active={active} />
      ) : (
        <button
          className="session-row"
          type="button"
          data-state={visual}
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
          {subagents.length ? (
            <span
              className="subagent-count"
              title={`${subagents.length} subagent${subagents.length === 1 ? "" : "s"}`}
            >
              {subagents.length}
            </span>
          ) : null}
          <span className="sr-only">{visual}</span>
          <ProviderMark provider={session.provider} small />
        </button>
      )}
      {!editing && onMenu ? (
        <div className="session-actions">
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
}: NavTreeProps) {
  const sessions = useSessionStore((state) => state.sessions);
  const projects = useWorkspaceStore((state) => state.projects);
  const worktrees = useWorkspaceStore((state) => state.worktrees);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const query = filter.trim().toLowerCase();
  const chats = useMemo(
    () =>
      sessions.filter((session) => {
        const project = projectForSession(session, projects);
        return !project && matches(session, null, query);
      }),
    [projects, query, sessions],
  );

  const toggle = (id: string) => {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const isOpen = (id: string) => query.length > 0 || !collapsed.has(id);
  const awaiting = sessions.filter((session) => session.awaiting).length;
  const summary = `${sessions.length} session${sessions.length === 1 ? "" : "s"} · ${awaiting} awaiting you`;

  const projectSessions = (project: Project): SessionSummary[] =>
    sessions.filter((session) => {
      const owner = projectForSession(session, projects);
      return owner?.id === project.id && matches(session, project, query);
    });

  const allProjectSessions = (project: Project): SessionSummary[] =>
    sessions.filter((session) => projectForSession(session, projects)?.id === project.id);

  const renderProject = (project: Project) => {
    const all = allProjectSessions(project);
    const shown = projectSessions(project);
    const projectMatches = !query || project.name.toLowerCase().includes(query);
    if (query && !projectMatches && shown.length === 0) return null;
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

    // Sessions that do not fall under any listed worktree stay directly under the project.
    const loose = project.isGit
      ? shown.filter((session) => !worktreeForSession(session, worktreeList))
      : shown;

    return (
      <div className="tree-group" key={project.id}>
        <div className="row-line">
          <Caret nodeId={`p-${project.id}`} label={`Toggle ${project.name}`} expanded={open} onToggle={toggle} />
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
            data-new-project={project.id}
            aria-label={`New session in ${project.name}`}
            onClick={() => onNewSession(project.id, null)}
          >
            <Icon name="plus" size="sm" />
          </button>
        </div>
        <div className="kids" id={`kids-p-${project.id}`} hidden={!open}>
          {project.isGit
            ? worktreeList.map((worktree) => {
                const inWorktree = shown.filter(
                  (session) => worktreeForSession(session, [worktree]) !== null,
                );
                const allInWorktree = all.filter(
                  (session) => worktreeForSession(session, [worktree]) !== null,
                );
                const worktreeMatches = !query || worktree.branch?.toLowerCase().includes(query) || projectMatches;
                if (query && !worktreeMatches && inWorktree.length === 0) return null;
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
                        <span className="node-name mono">
                          {worktree.branch ?? basename(worktree.path)}
                        </span>
                        {normalizePath(worktree.path) === normalizePath(project.path) ? (
                          <span className="node-count">checkout</span>
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
          {loose.length ? (
            loose.map((session) => (
              <SessionRow
                key={session.id}
                session={session}
                active={session.id === activeId}
                onSelect={onSelect}
                onMenu={onSessionMenu}
              />
            ))
          ) : project.isGit ? null : (
            <p className="tree-empty">No sessions yet</p>
          )}
          {project.isGit && loose.length === 0 && worktreeList.length === 0 ? (
            <p className="tree-empty">No worktrees</p>
          ) : null}
        </div>
      </div>
    );
  };

  const chatsOpen = isOpen("chats");
  const noMatches = query && chats.length === 0 && projects.every((project) => projectSessions(project).length === 0);

  return (
    <aside
      className={active ? "panel nav is-active" : "panel nav"}
      id="panel-nav"
      aria-label="Projects, worktrees and sessions"
    >
      <div className="nav-top">
        <button className="btn-ghost block" type="button" onClick={onAddProject}>
          <Icon name="folderPlus" size="sm" />
          Add project
        </button>
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
            placeholder="Filter tree"
            autoComplete="off"
            value={filter}
            onChange={(event) => onFilter(event.target.value)}
          />
        </div>
      </div>

      <div className="nav-scroll od-scroll">
        <div className="tree">
          <div className="tree-group">
            <div className="row-line">
              <Caret nodeId="chats" label="Toggle chats" expanded={chatsOpen} onToggle={toggle} />
              <button
                className="node-row is-root"
                type="button"
                aria-expanded={chatsOpen}
                aria-controls="kids-chats"
                onClick={() => toggle("chats")}
              >
                <span className="node-ico">
                  <Icon name="message" size="sm" />
                </span>
                <span className="node-name">Chats</span>
                <span className="node-count">{chats.length}</span>
              </button>
              <button className="node-add" type="button" aria-label="New chat" onClick={onNewChat}>
                <Icon name="plus" size="sm" />
              </button>
            </div>
            <div className="kids" id="kids-chats" hidden={!chatsOpen}>
              {chats.length ? (
                chats.map((session) => (
                  <SessionRow
                    key={session.id}
                    session={session}
                    active={session.id === activeId}
                    onSelect={onSelect}
                    onMenu={onSessionMenu}
                  />
                ))
              ) : (
                <p className="tree-empty">No chats yet</p>
              )}
            </div>
          </div>

          {projects.map(renderProject)}

          {noMatches ? (
            <p className="tree-empty" style={{ paddingLeft: 4 }}>
              Nothing matches “{filter}”.
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
