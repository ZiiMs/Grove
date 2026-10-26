import type { Decision, QuestionAnswer } from "@grove/contract";
import { useMemo, useState } from "react";
import { useBridge } from "../ipc/context";
import { useSessionStore } from "../state/sessionStore";
import { Icon } from "../workspace/Icon";
import { foldTranscript, type TranscriptItem } from "./transcript";
import { TranscriptRow } from "./SessionPane";

interface SubagentPaneProps {
  active: boolean;
  /** The parent session the worker ran inside. */
  sessionId: string;
  /** The parent's `subagent` tool call id, which prefixes every forwarded row. */
  callId: string;
  onClose: () => void;
}

function statusView(status: string | undefined): { visual: string; label: string } {
  switch (status) {
    case "running":
      return { visual: "running", label: "Running" };
    case "ok":
      return { visual: "done", label: "Done" };
    case "error":
      return { visual: "failed", label: "Failed" };
    case "cancelled":
      return { visual: "idle", label: "Cancelled" };
    default:
      return { visual: "idle", label: "Unknown" };
  }
}

/**
 * The read-only view of one worker. The worker has no conversation of its own in the store; what it
 * forwarded into the parent is what the transcript shows. Its question and approval cards carry the
 * parent's `sub-` request ids, so answering here reaches the live worker through the parent session.
 */
export function SubagentPane({ active, sessionId, callId, onClose }: SubagentPaneProps) {
  const bridge = useBridge();
  const detail = useSessionStore((state) => state.detail);
  const events = useSessionStore((state) => state.events);
  const sessions = useSessionStore((state) => state.sessions);
  const select = useSessionStore((state) => state.select);
  const respondPermission = useSessionStore((state) => state.respondPermission);
  const respondQuestion = useSessionStore((state) => state.respondQuestion);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const parent = sessions.find((row) => row.id === sessionId) ?? null;
  const subagent = parent?.subagents.find((row) => row.callId === callId) ?? null;
  const status = statusView(subagent?.status);
  const loaded = detail?.id === sessionId;
  // The stored summary is the parent's tool-result row: `subagent "..." finished.` plus the first
  // slice of the report. The worker's own messages are not persisted, so this is the only text.
  const report =
    subagent?.summary?.replace(/^subagent "[^"]*" finished\.\n\n/, "").trim() ?? "";

  const items = useMemo(() => {
    if (!loaded) return [] as TranscriptItem[];
    const prefix = `sub-${callId}-`;
    const owned = events.filter((event) => {
      switch (event.kind) {
        case "tool_call":
        case "tool_result":
          return event.call_id === callId || event.call_id.startsWith(prefix);
        case "permission_request":
        case "permission_resolved":
        case "question_request":
        case "question_resolved":
          return event.request_id.startsWith(prefix);
        default:
          return false;
      }
    });
    return foldTranscript(owned).map((item) =>
      item.kind === "tool"
        ? { ...item, title: item.title.replace(/^subagent · /, "") }
        : item,
    );
  }, [callId, events, loaded]);

  const toggle = (id: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const decide = (requestId: string, decision: Decision) => {
    void respondPermission(bridge, requestId, decision);
  };
  const answer = (requestId: string, answers: QuestionAnswer[]) =>
    respondQuestion(bridge, requestId, answers);

  return (
    <main
      className={active ? "panel session is-active" : "panel session"}
      id="panel-session"
      aria-label="Subagent transcript"
    >
      <div className="session-head subagent-head">
        <span className="node-ico" aria-hidden="true">
          <Icon name="sparkle" size="sm" />
        </span>
        <div className="subagent-title">
          <h1 className="sh-title" title={subagent?.description ?? "Subagent"}>
            {subagent?.description ?? "Subagent"}
          </h1>
          <span className="sh-sub">worker in {parent?.title ?? "session"}</span>
        </div>
        <span className="sh-state" data-state={status.visual}>
          <span className="dot" data-state={status.visual} aria-hidden="true" />
          <span className="sh-state-label">{status.label}</span>
        </span>
        <div className="sh-right">
          <button className="btn-ghost" type="button" onClick={onClose}>
            <Icon name="chevronLeft" size="sm" />
            Session
          </button>
        </div>
      </div>

      <div className="run-scroll od-scroll">
        <div className="run-inner">
          {loaded && report ? (
            <section className="subagent-report" aria-label="Worker report">
              <span className="subagent-report-label">Report</span>
              <pre>{report}</pre>
            </section>
          ) : null}
          {!loaded ? (
            <div className="empty">
              <strong>Opening the parent session</strong>
              <p className="subagent-empty-note">The worker's rows are read from that transcript.</p>
              <button
                className="btn-primary"
                type="button"
                onClick={() => void select(bridge, sessionId)}
              >
                Open {parent?.title ?? "session"}
              </button>
            </div>
          ) : items.length ? (
            <div className="subagent-rows">
              {items.map((item) => (
                <div className="subagent-item" key={item.id}>
                  <TranscriptRow
                    item={item}
                    footer={false}
                    open={expanded.has(item.id)}
                    onToggle={() => toggle(item.id)}
                    onDecide={decide}
                    onAnswer={answer}
                  />
                </div>
              ))}
            </div>
          ) : (
            <div className="empty">
              <strong>No forwarded activity yet</strong>
              This worker has not produced a tool call, question or report that reached its parent.
            </div>
          )}
        </div>
      </div>

      <footer className="subagent-foot">
        <p>
          Read-only worker view. The report above is the stored summary; the worker's own messages
          are not kept. Questions and approvals are answerable here, and the full report went to the
          parent model.
        </p>
      </footer>
    </main>
  );
}
