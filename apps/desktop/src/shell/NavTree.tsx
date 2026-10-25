import type { Project, SessionSummary } from "@grove/contract";
import { useMemo, useState } from "react";
import { useSessionStore } from "../state/sessionStore";
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
  ]
    .join(" ")
    .toLowerCase();
  return haystack.includes(query);
}

function SessionRow({
  session,
  active,
  onSelect,
}: {
  session: SessionSummary;
  active: boolean;
  onSelect: (id: string) => void;
}) {
  const visual = sessionVisual(session);
  return (
    <button
      className="session-row"
      type="button"
      data-state={visual}
      aria-current={active ? "true" : undefined}
      onClick={() => onSelect(session.id)}
      title={session.title}
    >
      <span className="dot" data-state={visual} aria-hidden="true" />
      <span className="s-title">{session.title}</span>
      <span className="sr-only">{visual}</span>
      <ProviderMark provider={session.provider} small />
    </button>
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
