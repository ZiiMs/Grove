import { useEffect, useState } from "react";
import { useSessionStore } from "../state/sessionStore";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore } from "../state/workspaceStore";
import { projectForSession } from "../workspace/tree";
import { SessionPane } from "./SessionPane";
import { SubagentPane } from "./SubagentPane";

/**
 * The centre column: the active conversation, or the worker view selected from the tree. Keeping the
 * switch here leaves App as the shell and the tree free to open a worker without threading a second
 * selection callback through the shared seam.
 */
export function SessionSurface({ active }: { active: boolean }) {
  const activeId = useSessionStore((state) => state.activeId);
  const sessions = useSessionStore((state) => state.sessions);
  const projects = useWorkspaceStore((state) => state.projects);
  const subagent = useUiStore((state) => state.subagent);
  const closeSubagent = useUiStore((state) => state.closeSubagent);
  // Opening a worker starts by selecting its parent; stay on the worker pane while that lands.
  const [workerVisible, setWorkerVisible] = useState(false);

  const summary = sessions.find((row) => row.id === activeId) ?? null;
  const project = summary ? projectForSession(summary, projects) : null;

  useEffect(() => {
    if (!subagent) {
      setWorkerVisible(false);
      return;
    }
    if (activeId === subagent.sessionId) {
      setWorkerVisible(true);
      return;
    }
    if (workerVisible) closeSubagent();
  }, [activeId, closeSubagent, subagent, workerVisible]);

  if (subagent) {
    return (
      <SubagentPane
        active={active}
        sessionId={subagent.sessionId}
        callId={subagent.callId}
        onClose={closeSubagent}
      />
    );
  }
  return <SessionPane active={active} summary={summary} project={project} />;
}
