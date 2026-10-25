/**
 * PROTOTYPE — throwaway. Left navigation as a true tree: every row indents by one tab per level and
 * draws its parent connector, so hierarchy is visible rather than implied by padding.
 */

import { useState } from "react";
import { looseChats, projects, type NavSession } from "../fixtures";

interface IndentsProps {
  depth: number;
}

/** One cell per ancestor level. The last cell draws the elbow; the rest run straight through. */
function Indents({ depth }: IndentsProps) {
  return (
    <>
      {Array.from({ length: depth }).map((_, index) => (
        <span
          key={index}
          className={index === depth - 1 ? "pv-tree-indent is-last" : "pv-tree-indent"}
          aria-hidden="true"
        />
      ))}
    </>
  );
}

function Caret({ open, onClick }: { open: boolean; onClick: () => void }) {
  return (
    <span
      className={open ? "pv-tree-caret is-open" : "pv-tree-caret"}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
      role="button"
      tabIndex={-1}
      aria-hidden="true"
    >
      ▸
    </span>
  );
}

function stateClass(state: string) {
  if (state === "live") return "pv-status-dot is-live";
  if (state === "warn") return "pv-status-dot is-warn";
  if (state === "err") return "pv-status-dot is-err";
  return "pv-status-dot";
}

function SessionRow({ session, depth, active }: { session: NavSession; depth: number; active: boolean }) {
  return (
    <div className={active ? "pv-tree-row is-selected" : "pv-tree-row"} role="treeitem" aria-selected={active}>
      <Indents depth={depth} />
      <span className="pv-tree-gutter" />
      <span className={stateClass(session.state)} />
      <span className="pv-tree-label">{session.title}</span>
      <span className="pv-tree-meta">{session.elapsed}</span>
    </div>
  );
}

export function NavTree({
  activeSession = "s1",
  onSettings,
  onSkills,
  onMcp,
}: {
  activeSession?: string;
  onSettings?: () => void;
  onSkills?: () => void;
  onMcp?: () => void;
}) {
  const [open, setOpen] = useState<Record<string, boolean>>({
    chats: true,
    grove: true,
    thinkrail: true,
    "workspace-3": true,
    "acp-root": false,
    "tr-main": false,
  });

  const toggle = (id: string) => setOpen((current) => ({ ...current, [id]: !current[id] }));

  return (
    <nav className="pv-nav" aria-label="Projects">
      <div className="pv-nav-head">
        <span className="pv-brand-mark">◆</span>
        <span className="pv-brand-word">Grove</span>
        <span className="pv-rail-spacer" style={{ flex: 1 }} />
        <button type="button" className="pv-nav-action" title="Add project" aria-label="Add project">
          ＋
        </button>
        <button
          type="button"
          className="pv-nav-action"
          title="New chat — no project directory"
          aria-label="New chat without a project"
        >
          ⌯
        </button>
        <span className="pv-nav-sep" aria-hidden="true" />
        <button type="button" className="pv-nav-action" title="Skill catalog" aria-label="Skill catalog" onClick={onSkills}>
          ✦
        </button>
        <button type="button" className="pv-nav-action" title="MCP servers" aria-label="MCP servers" onClick={onMcp}>
          ⬡
        </button>
      </div>

      <div className="pv-nav-scroll" role="tree">
        <div className="pv-nav-cap">
          <Caret open={open.chats} onClick={() => toggle("chats")} />
          Chats
          <span className="pv-nav-count">{looseChats.length}</span>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <span className="pv-nav-hint">no directory</span>
        </div>
        {open.chats
          ? looseChats.map((chat) => (
              <SessionRow key={chat.id} session={chat} depth={0} active={chat.id === activeSession} />
            ))
          : null}

        <div className="pv-nav-cap">
          <span className="pv-tree-caret is-open" aria-hidden="true">▸</span>
          Projects
          <span className="pv-nav-count">{projects.length}</span>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <button type="button" className="pv-nav-action" title="Add project" aria-label="Add project in list">
            ＋
          </button>
        </div>

        {projects.map((project) => (
          <div key={project.id} role="treeitem" aria-expanded={open[project.id]}>
            <div
              className="pv-tree-row is-project"
              onClick={() => toggle(project.id)}
            >
              <Caret open={!!open[project.id]} onClick={() => toggle(project.id)} />
              <span className="pv-tree-label">{project.name}</span>
              <span className="pv-tree-meta pv-mono">{project.path}</span>
              <span className="pv-nav-count">{project.worktrees.length}</span>
            </div>

            {open[project.id]
              ? project.worktrees.map((worktree) => (
                  <div key={worktree.id} role="treeitem" aria-expanded={open[worktree.id]}>
                    <div
                      className="pv-tree-row is-worktree"
                      onClick={() => toggle(worktree.id)}
                    >
                      <Indents depth={1} />
                      <Caret open={!!open[worktree.id]} onClick={() => toggle(worktree.id)} />
                      <span className={stateClass(worktree.state)} />
                      <span className="pv-tree-label pv-mono">{worktree.name}</span>
                      <span className="pv-tree-branch pv-mono">{worktree.branch}</span>
                    </div>

                    {open[worktree.id]
                      ? worktree.sessions.map((session) => (
                          <SessionRow
                            key={session.id}
                            session={session}
                            depth={2}
                            active={session.id === activeSession}
                          />
                        ))
                      : null}

                    {open[worktree.id] && worktree.sessions.length === 0 ? (
                      <div className="pv-tree-row is-empty">
                        <Indents depth={2} />
                        <span className="pv-tree-gutter" />
                        <span className="pv-tree-label">no sessions</span>
                        <span className="pv-tree-meta">＋</span>
                      </div>
                    ) : null}
                  </div>
                ))
              : null}
          </div>
        ))}
      </div>

      <div className="pv-nav-foot">
        <button
          type="button"
          className="pv-nav-settings"
          title="Settings"
          aria-label="Settings"
          onClick={onSettings}
        >
          ⚙
        </button>
      </div>
    </nav>
  );
}
