import type {
  AppInfo,
  PermissionMode,
  SessionEvent,
  Project,
  SessionDetail,
  SessionSummary,
} from "@grove/contract";
import type { Preferences } from "../state/workspaceStore";
import { providerMeta, sessionVisual } from "../workspace/models";
import { basename } from "../workspace/tree";

interface StatusBarProps {
  info: AppInfo | null;
  sessions: SessionSummary[];
  session: SessionDetail | null;
  project: Project | null;
  branch: string | null;
  events: SessionEvent[];
  plan: { done: number; total: number };
  prefs: Preferences;
}

const PERM_LABEL: Record<PermissionMode, string> = {
  ask: "ask",
  full: "full access",
};

/** The 26px footer: live counts on the left, where you are in the middle, settings state right. */
export function StatusBar({
  info,
  sessions,
  session,
  project,
  branch,
  events,
  plan,
  prefs,
}: StatusBarProps) {
  const running = sessions.filter((row) => sessionVisual(row) === "running").length;
  const waiting = sessions.filter((row) => row.awaiting && !row.archivedAt).length;
  const failed = sessions.filter((row) => sessionVisual(row) === "failed").length;
  const bits = [`${running} running`, `${waiting} awaiting you`];
  if (failed) bits.push(`${failed} failed`);
  const dot = waiting ? "waiting" : running ? "running" : session ? sessionVisual(session) : "idle";
  const provider = session ? providerMeta(session.provider) : null;
  const tools = events.filter((event) => event.kind === "tool_call").length;
  const turns = events.filter(
    (event) => event.kind === "message" && event.role !== "system",
  ).length;
  const where = project ? `${project.name} · ${branch ?? "no worktree"}` : "chats · no worktree";
  const perm = session?.permissionMode ?? (prefs.permissionMode === "bypass" ? "full" : "ask");

  return (
    <footer className="statusbar">
      <div className="sb-left">
        <span className="dot" data-state={dot} aria-hidden="true" />
        <span id="status-live">{bits.join(" · ")}</span>
      </div>
      <div className="sb-mid" id="status-mid">
        {session
          ? `${where} · ${provider?.name} · ${turns} turn${turns === 1 ? "" : "s"} · ${tools} tool call${tools === 1 ? "" : "s"}`
          : where}
      </div>
      <div className="sb-right">
        <span className="sb-item tidy">
          perm <span>{PERM_LABEL[perm]}</span>
        </span>
        <span className="sb-item tidy">
          plan <span>{plan.total ? `${plan.done}/${plan.total}` : "—"}</span>
        </span>
        <span className="sb-item mono">
          {info ? `${basename(info.name).toLowerCase()} ${info.version}` : "core offline"}
        </span>
      </div>
    </footer>
  );
}
