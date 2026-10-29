import type { GitWorktree, McpDiscovery, PlanUsage, Project, SubagentSummary } from "@grove/contract";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useBridge } from "../ipc/context";
import { useContextStore } from "../state/contextStore";
import { useSessionStore } from "../state/sessionStore";
import { Icon } from "../workspace/Icon";
import { ProviderGlyph } from "../workspace/ProviderGlyph";
import { modelLabel } from "../workspace/modelLabel";
import { providerMeta } from "../workspace/models";
import { thinkingLabel } from "../workspace/thinking";

/**
 * The floating right-hand card: everything about the active session that is not the transcript.
 * It is an overlay, not a grid column, and the shell shows it only while the context column is
 * closed, so the two right sidebars never stack.
 *
 * Costs and provider quota windows are deliberately absent: Grove records tokens, not prices.
 */
export function SessionInfoPanel({
  project,
  worktree,
  branch,
  plan,
  onClose,
}: {
  project: Project | null;
  worktree: GitWorktree | null;
  branch: string | null;
  plan: { done: number; total: number };
  onClose: () => void;
}) {
  const bridge = useBridge();
  const detail = useSessionStore((state) => state.detail);
  const events = useSessionStore((state) => state.events);
  const context = useSessionStore((state) => state.context);
  const sessions = useSessionStore((state) => state.sessions);
  const git = useContextStore((state) => state.git);
  const gitStats = useContextStore((state) => state.gitStats);
  const [mcp, setMcp] = useState<McpDiscovery | null>(null);
  const [planUsage, setPlanUsage] = useState<PlanUsage | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);

  // Discovery reads config files; it does not spawn or probe a server.
  useEffect(() => {
    let active = true;
    void bridge
      .invoke("mcp_servers")
      .then((found) => {
        if (active) setMcp(found);
      })
      .catch(() => {
        if (active) setMcp({ servers: [], warnings: [] });
      });
    return () => {
      active = false;
    };
  }, [bridge]);

  // Plan usage is a provider endpoint: only OpenCode Go has one in this build, so don't ask others.
  useEffect(() => {
    if (detail?.provider !== "opencode-go") {
      setPlanUsage(null);
      setPlanError(null);
      return;
    }
    let active = true;
    setPlanUsage(null);
    setPlanError(null);
    void bridge
      .invoke("plan_usage", { provider: detail.provider })
      .then((usage) => {
        if (active) setPlanUsage(usage);
      })
      .catch(() => {
        if (active) setPlanError("Plan usage unavailable");
      });
    return () => {
      active = false;
    };
  }, [bridge, detail?.provider]);

  const usage = useMemo(
    () =>
      events.reduce(
        (total, event) =>
          event.kind === "usage"
            ? {
                input: total.input + event.input_tokens,
                output: total.output + event.output_tokens,
              }
            : total,
        { input: 0, output: 0 },
      ),
    [events],
  );

  const turns = useMemo(() => {
    let user = 0;
    let assistant = 0;
    let tools = 0;
    let diffs = 0;
    for (const event of events) {
      if (event.kind === "message") {
        if (event.role === "user") user += 1;
        else if (event.role === "assistant") assistant += 1;
      } else if (event.kind === "tool_call") {
        tools += 1;
      } else if (event.kind === "diff") {
        diffs += 1;
      }
    }
    const first = events[0] ? Date.parse(events[0].at) : null;
    const last = events.length ? Date.parse(events[events.length - 1].at) : null;
    return { user, assistant, tools, diffs, elapsed: first && last ? last - first : 0 };
  }, [events]);

  const subagents = sessions.find((row) => row.id === detail?.id)?.subagents ?? [];
  const meta = providerMeta(detail?.provider ?? "mock");
  const model = detail?.model
    ? modelLabel(detail.model, detail.provider)
    : `${meta.name} default`;
  const contextPercent =
    context && context.limits.totalChars > 0
      ? Math.min(100, Math.round((context.totalChars / context.limits.totalChars) * 100))
      : 0;

  return (
    <aside className="session-info" aria-label="Session info">
      <div className="si-head">
        <div className="si-heading">
          <h2>Session</h2>
          <p title={detail?.title}>{detail?.title ?? "No session open"}</p>
        </div>
        <button className="icon-btn sm" type="button" aria-label="Close session info" onClick={onClose}>
          <Icon name="x" size="sm" />
        </button>
      </div>

      <div className="si-scroll">
        {detail ? (
          <Section title="Model">
            <div className="si-model">
              <ProviderGlyph provider={detail.provider} />
              <span className="si-model-name" title={detail.model ?? undefined}>
                {model}
              </span>
              <span className="si-model-provider">{meta.name}</span>
            </div>
            <Row label="Status" value={detail.status} />
            <Row label="Thinking" value={thinkingLabel(detail.thinkingLevel ?? "off")} />
            <Row
              label="Approvals"
              value={detail.permissionMode === "full" ? "Full access" : "Ask before changes"}
            />
            {detail.model ? <p className="si-path">{detail.model}</p> : null}
          </Section>
        ) : null}

        <Section title="Context">
          {context ? (
            <>
              <div className="si-bar" aria-hidden="true">
                <span style={{ width: `${contextPercent}%` }} />
              </div>
              <Row
                label={`${formatChars(context.totalChars)} / ${formatChars(context.limits.totalChars)} chars`}
                value={`${contextPercent}%`}
              />
              <Row label="Pinned specs" value={String(context.pins.length)} />
              <Row label="Plan" value={plan.total ? `${plan.done} / ${plan.total}` : "none"} />
            </>
          ) : (
            <p className="si-empty">No context snapshot yet.</p>
          )}
        </Section>

        <Section title="Project">
          {project ? (
            <>
              <Row label="Project" value={project.name} />
              <Row label="Branch" value={branch ?? "no worktree"} />
              <Row label="Changed files" value={String(git?.files.length ?? 0)} />
              <div className="si-row">
                <span>Diff</span>
                <span>
                  <span className="si-add">+{gitStats?.added ?? 0}</span>{" "}
                  <span className="si-del">−{gitStats?.removed ?? 0}</span>
                </span>
              </div>
              <p className="si-path" title={worktree?.path ?? detail?.cwd}>
                {worktree?.path ?? detail?.cwd ?? project.path}
              </p>
            </>
          ) : (
            <p className="si-empty">This session is not attached to a project.</p>
          )}
        </Section>

        {detail?.provider === "opencode-go" ? (
          <Section title="Plan usage">
            {planUsage ? (
              <div className="si-plan">
                {planUsage.windows.map((window) => (
                  <div className="si-plan-row" key={window.id}>
                    <div className="si-plan-head">
                      <span>{window.label}</span>
                      <span>
                        {window.status === "rate-limited"
                          ? "Limit reached"
                          : `${window.remainingPercent}% left`}
                      </span>
                    </div>
                    <div
                      className="si-bar"
                      data-low={window.remainingPercent <= 20 ? "true" : undefined}
                    >
                      <span style={{ width: `${window.remainingPercent}%` }} />
                    </div>
                    <p className="si-plan-reset">
                      {window.status === "rate-limited"
                        ? "No budget left in this window"
                        : `resets ${resetLabel(window.resetsAt)}`}
                    </p>
                  </div>
                ))}
              </div>
            ) : (
              <p className="si-empty">{planError ?? "Loading plan usage…"}</p>
            )}
          </Section>
        ) : null}

        <Section title="This session">
          {usage.input + usage.output > 0 ? (
            <>
              <Row label="Input" value={formatTokens(usage.input)} />
              <Row label="Output" value={formatTokens(usage.output)} />
              <Row label="Total" value={formatTokens(usage.input + usage.output)} />
            </>
          ) : (
            <p className="si-empty">No usage recorded yet.</p>
          )}
          <Row label="Messages" value={`${turns.user} user · ${turns.assistant} assistant`} />
          <Row label="Tool calls" value={String(turns.tools)} />
          <Row label="Diffs" value={String(turns.diffs)} />
          <Row label="Elapsed" value={formatDuration(turns.elapsed)} />
        </Section>

        <Section
          title="Subagents"
          aside={<span className="si-count">{subagents.length}</span>}
        >
          {subagents.length ? (
            subagents.map((subagent) => (
              <SubagentRow key={subagent.callId} subagent={subagent} />
            ))
          ) : (
            <p className="si-empty">No subagents.</p>
          )}
        </Section>

        <Section
          title="MCP"
          aside={<span className="si-count">{mcp ? mcp.servers.length : "…"}</span>}
        >
          {mcp ? (
            mcp.servers.length ? (
              mcp.servers.map((server) => (
                <Row
                  key={`${server.source}:${server.name}`}
                  label={server.name}
                  value={server.transport}
                />
              ))
            ) : (
              <p className="si-empty">No MCP servers found.</p>
            )
          ) : (
            <p className="si-empty">Reading server configs…</p>
          )}
        </Section>
      </div>
    </aside>
  );
}

function Section({
  title,
  aside,
  children,
}: {
  title: string;
  aside?: ReactNode;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(true);
  return (
    <section className="si-section">
      <button
        className="si-title"
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <span className="si-chevron">
          <Icon name="chevronRight" size="xs" />
        </span>
        <span className="si-title-text">{title}</span>
        {aside}
      </button>
      {open ? <div className="si-body">{children}</div> : null}
    </section>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="si-row">
      <span title={label}>{label}</span>
      <span>{value}</span>
    </div>
  );
}

function SubagentRow({ subagent }: { subagent: SubagentSummary }) {
  const state =
    subagent.status === "running"
      ? "running"
      : subagent.status === "error"
        ? "failed"
        : "idle";
  return (
    <div className="si-sub">
      <span className="dot" data-state={state} aria-hidden="true" />
      <span className="si-sub-name" title={subagent.description}>
        {subagent.description}
      </span>
      <span className="si-sub-status">{subagent.status}</span>
    </div>
  );
}

function formatChars(value: number): string {
  return value.toLocaleString("en-US");
}

function formatTokens(value: number): string {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}m`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}k`;
  return String(value);
}

function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "0s";
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  if (minutes < 60) return rest ? `${minutes}m ${rest}s` : `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const mins = minutes % 60;
  return mins ? `${hours}h ${mins}m` : `${hours}h`;
}

function resetLabel(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "soon";
  const today = new Date();
  const sameDay = date.toDateString() === today.toDateString();
  return date.toLocaleString(
    undefined,
    sameDay
      ? { hour: "numeric", minute: "2-digit" }
      : { weekday: "short", month: "short", day: "numeric", hour: "numeric", minute: "2-digit" },
  );
}
