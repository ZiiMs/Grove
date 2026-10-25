import type { Decision, Project, SessionSummary } from "@grove/contract";
import { useEffect, useMemo, useRef, useState } from "react";

const EMPTY_QUEUE: string[] = [];
import { useVirtualizer } from "@tanstack/react-virtual";
import { useBridge } from "../ipc/context";
import { errorText } from "../ipc/errors";
import { useSessionStore } from "../state/sessionStore";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore } from "../state/workspaceStore";
import {
  availableProviderIds,
  defaultModelFor,
  firstAvailableProvider,
  providerMeta,
  turnInFlight,
  modelGroupsFor,
  sessionVisual,
  SESSION_UI_STATE,
} from "../workspace/models";
import { Icon } from "../workspace/Icon";
import {
  clockLabel,
  durationLabel,
  foldTranscript,
  type TranscriptItem,
} from "../session/transcript";

interface SessionPaneProps {
  active: boolean;
  summary: SessionSummary | null;
  project: Project | null;
}

function Mark({ provider, small }: { provider: string; small?: boolean }) {
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

/** The centre: compact header, the transcript, and the composer. */
export function SessionPane({ active, summary, project }: SessionPaneProps) {
  const bridge = useBridge();
  const detail = useSessionStore((state) => state.detail);
  const events = useSessionStore((state) => state.events);
  const context = useSessionStore((state) => state.context);
  const openSettings = useUiStore((state) => state.openSettings);
  const resume = useSessionStore((state) => state.resume);
  const respondPermission = useSessionStore((state) => state.respondPermission);
  const busy = useSessionStore((state) => state.busy);
  const error = useSessionStore((state) => state.error);
  const applyContext = useSessionStore((state) => state.applyContext);
  const notify = useUiStore((state) => state.notify);
  const openNewSession = useUiStore((state) => state.openNewSession);

  const items = useMemo(() => foldTranscript(events), [events]);
  const live = detail ? detail.status !== "exited" && detail.status !== "error" : false;
  const working = turnInFlight(summary ?? detail);
  const visual = sessionVisual(summary ?? detail);
  const provider = detail ? providerMeta(detail.provider) : null;

  const headerActions = (
    <div className="sh-right">
      {detail && !live ? (
        <button
          className="btn-ghost sm"
          type="button"
          disabled={busy}
          onClick={() => void resume(bridge)}
          title="Resume this session"
        >
          Resume
        </button>
      ) : null}
      <button
        className="icon-btn"
        type="button"
        onClick={() => openSettings("skills")}
        title="Skills"
        aria-label="Open skills"
      >
        <Icon name="book" />
      </button>
    </div>
  );

  const pinned = context?.pins ?? [];

  return (
    <main
      className={active ? "panel session is-active" : "panel session"}
      id="panel-session"
      aria-label="Active session"
    >
      <div className="session-head">
        {detail ? (
          <>
            <Mark provider={detail.provider} />
            <h1 className="sh-title" title={detail.title}>
              {detail.title}
            </h1>
            <span className="sh-state" data-state={visual}>
              <span className="dot" data-state={visual} aria-hidden="true" />
              <span className="sh-state-label">{SESSION_UI_STATE[visual]}</span>
            </span>
          </>
        ) : (
          <h1 className="sh-title">No session selected</h1>
        )}
        {headerActions}
      </div>

      <Transcript
        items={items}
        working={working}
        providerName={provider?.name ?? "the provider"}
        onDecide={(requestId, decision) => void respondPermission(bridge, requestId, decision)}
        empty={!detail}
        onNewSession={() => openNewSession({ projectId: project?.id ?? null })}
      />

      {detail ? (
        <Composer
          sessionId={detail.id}
          model={detail.model}
          pinned={pinned}
          onUnpin={async (specId) => {
            try {
              const next = await bridge.invoke("session_pin", {
                id: detail.id,
                specId,
                pinned: false,
              });
              applyContext(next);
            } catch (err) {
              notify(errorText(err), "error");
            }
          }}
        />
      ) : null}

      {error ? (
        <p className="session-error" role="alert" style={{ padding: "0 20px 8px" }}>
          {error}
        </p>
      ) : null}
    </main>
  );
}

function Transcript({
  items,
  working,
  providerName,
  onDecide,
  empty,
  onNewSession,
}: {
  items: TranscriptItem[];
  working: boolean;
  providerName: string;
  onDecide: (requestId: string, decision: Decision) => void;
  empty: boolean;
  onNewSession: () => void;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 120,
    overscan: 6,
    // jsdom has no layout engine; a sane initial box means tests render the first rows, and the
    // browser's first paint takes over from the same starting point.
    initialRect: { width: 0, height: 640 },
  });

  useEffect(() => {
    const node = scrollRef.current;
    if (!node) return;
    if (stick.current) node.scrollTop = node.scrollHeight;
  }, [items, working]);

  const onScroll = () => {
    const node = scrollRef.current;
    if (!node) return;
    stick.current = node.scrollHeight - node.scrollTop - node.clientHeight < 80;
  };

  const toggle = (id: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  return (
    <div
      className="run-scroll od-scroll"
      ref={scrollRef}
      onScroll={onScroll}
      tabIndex={0}
      role="region"
      aria-label="Session transcript"
    >
      <div className="run-inner">
        {empty ? (
          <div className="empty">
            <strong>Nothing here yet</strong>
            Select a session from the tree, or start a new one to give {providerName} its first
            instruction.
            <div style={{ marginTop: 12 }}>
              <button className="btn-primary" type="button" onClick={onNewSession}>
                <Icon name="plus" size="sm" /> New session
              </button>
            </div>
          </div>
        ) : items.length === 0 ? (
          <div className="empty">
            <strong>Nothing here yet</strong>
            This session is connected to {providerName} and has not run a turn. Send the first
            instruction below — its transcript, tool calls and approvals stay with this session.
          </div>
        ) : (
          <div className="run-rows" style={{ height: virtualizer.getTotalSize() }}>
            {virtualizer.getVirtualItems().map((row) => {
              const item = items[row.index];
              return (
                <div
                  key={row.key}
                  className="run-row"
                  data-index={row.index}
                  ref={virtualizer.measureElement}
                  style={{ transform: `translateY(${row.start}px)` }}
                >
                  <TranscriptRow
                    item={item}
                    open={expanded.has(item.id)}
                    onToggle={() => toggle(item.id)}
                    onDecide={onDecide}
                  />
                </div>
              );
            })}
          </div>
        )}
        {working ? (
          <div className="typing" aria-hidden="true">
            <i />
            <i />
            <i />
          </div>
        ) : null}
      </div>
    </div>
  );
}

function AssistantHead({
  provider,
  at,
  durationMs,
}: {
  provider: string;
  at: string;
  durationMs?: number | null;
}) {
  return (
    <div className="turn-head">
      <Mark provider={provider} small />
      <strong>{providerMeta(provider).name}</strong>
      <span className="meta">{durationLabel(durationMs ?? null) || clockLabel(at)}</span>
    </div>
  );
}

function TranscriptRow({
  item,
  open,
  onToggle,
  onDecide,
}: {
  item: TranscriptItem;
  open: boolean;
  onToggle: () => void;
  onDecide: (requestId: string, decision: Decision) => void;
}) {
  const [answered, setAnswered] = useState<Decision | null>(null);
  const detail = useSessionStore((state) => state.detail);
  const provider = detail?.provider ?? "mock";

  switch (item.kind) {
    case "message": {
      if (item.role === "user") {
        const text = item.blocks
          .filter((block) => block.type === "text")
          .map((block) => block.text)
          .join("\n");
        return (
          <div className="turn turn-user">
            <div className="bubble">
              <p>{text}</p>
            </div>
          </div>
        );
      }
      return (
        <div className="turn">
          <AssistantHead provider={provider} at={item.at} durationMs={item.durationMs} />
          {item.blocks.map((block, index) => {
            if (block.type === "code") {
              return (
                <pre className="turn-code" key={index}>
                  {block.text}
                </pre>
              );
            }
            if (block.type === "thinking") {
              return (
                <p className="turn-thinking" key={index}>
                  {block.text}
                </p>
              );
            }
            return <p key={index}>{block.text}</p>;
          })}
        </div>
      );
    }
    case "stream":
      return (
        <div className="turn">
          <AssistantHead provider={provider} at={item.at} />
          <p>
            {item.text}
            <span className="stream-caret" aria-hidden="true" />
          </p>
        </div>
      );
    case "tool": {
      const icon =
        /read|grep|search/i.test(item.name)
          ? "search"
          : /write/i.test(item.name)
            ? "file"
            : /edit/i.test(item.name)
              ? "edit"
              : "terminal";
      return (
        <div className="turn">
          <div className="tool" data-status={item.status ?? "running"}>
            <button
              className="tool-head"
              type="button"
              aria-expanded={open}
              onClick={onToggle}
            >
              <span className="tool-ico">
                <Icon name={icon} size="sm" />
              </span>
              <span className="tool-name">{item.name}</span>
              <span className="tool-target">{item.title}</span>
              <span className="tool-time">
                {item.status === null ? "running" : durationLabel(item.durationMs)}
              </span>
              <span className="chev">
                <Icon name="chevron" size="sm" />
              </span>
            </button>
            {open ? (
              <div className="tool-body">
                {item.title ? <span className="tool-path">{item.title}</span> : null}
                <pre className={item.status === "error" ? "err" : undefined}>
                  {item.summary ?? "No output recorded."}
                </pre>
              </div>
            ) : null}
          </div>
        </div>
      );
    }
    case "diff":
      return (
        <div className="turn">
          <div className="tool" data-status="ok">
            <button className="tool-head" type="button" aria-expanded={open} onClick={onToggle}>
              <span className="tool-ico">
                <Icon name="diff" size="sm" />
              </span>
              <span className="tool-name">Diff</span>
              <span className="tool-target">{item.path}</span>
              <span className="tool-time">
                <span className="add">+{item.added}</span> <span className="del">−{item.removed}</span>
              </span>
              <span className="chev">
                <Icon name="chevron" size="sm" />
              </span>
            </button>
            {open ? (
              <div className="tool-body">
                <span className="tool-path">{item.path}</span>
                <DiffBody patch={item.patch} />
              </div>
            ) : null}
          </div>
        </div>
      );
    case "permission":
      return (
        <div className="turn">
          <div className="perm" role="group" aria-label="Permission request">
            <div className="perm-head">
              <Icon name="alert" size="sm" />
              Permission requested
            </div>
            <p className="perm-cmd">{item.title}</p>
            {item.detail ? <p className="perm-note">{item.detail}</p> : null}
            {item.decision === null ? (
              <div className="perm-actions">
                <button
                  className="btn-primary"
                  type="button"
                  disabled={answered !== null}
                  onClick={() => {
                    setAnswered("approve");
                    onDecide(item.requestId, "approve");
                  }}
                >
                  Approve and run
                </button>
                <button
                  className="btn-ghost"
                  type="button"
                  disabled={answered !== null}
                  onClick={() => {
                    setAnswered("deny");
                    onDecide(item.requestId, "deny");
                  }}
                >
                  Reject
                </button>
              </div>
            ) : (
              <p className="perm-note">{item.decision === "approve" ? "Approved" : "Rejected"}</p>
            )}
          </div>
        </div>
      );
    case "notice":
      return (
        <div className="turn">
          <div className={item.tone === "error" ? "turn-note danger" : "turn-note"}>
            <span className="n-ico">
              <Icon name={item.tone === "error" ? "alert" : "checkCircle"} size="sm" />
            </span>
            <span>{item.text}</span>
          </div>
        </div>
      );
    case "usage":
      return null;
  }
}

function DiffBody({ patch }: { patch: string }) {
  return (
    <div className="diff-lines">
      {patch.split("\n").map((line, index) => {
        const kind = line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : line.startsWith("@@") ? "hunk" : "ctx";
        return (
          <div key={index} className={`dl ${kind === "ctx" ? "" : kind}`}>
            <span className="dl-sign">
              {kind === "add" ? "+" : kind === "del" ? "−" : " "}
            </span>
            <span className="dl-code">{line.replace(/^[+-]/, "") || " "}</span>
          </div>
        );
      })}
    </div>
  );
}

function Composer({
  sessionId,
  model,
  pinned,
  onUnpin,
}: {
  sessionId: string;
  model: string | null;
  pinned: { specId: string; title: string }[];
  onUnpin: (specId: string) => void;
}) {
  const bridge = useBridge();
  const draft = useSessionStore((state) => state.drafts[sessionId] ?? "");
  const queue = useSessionStore((state) => state.queues[sessionId]) ?? EMPTY_QUEUE;
  const detail = useSessionStore((state) => state.detail);
  const setDraft = useSessionStore((state) => state.setDraft);
  const send = useSessionStore((state) => state.send);
  const enqueue = useSessionStore((state) => state.enqueue);
  const dequeue = useSessionStore((state) => state.dequeue);
  const cancel = useSessionStore((state) => state.cancel);
  const switchProvider = useSessionStore((state) => state.switchProvider);
  const resume = useSessionStore((state) => state.resume);
  const busy = useSessionStore((state) => state.busy);
  const prefs = useWorkspaceStore((state) => state.prefs);
  const registered = useSessionStore((state) => state.providers);
  const providerStatuses = useSessionStore((state) => state.providerStatuses);
  const notify = useUiStore((state) => state.notify);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  const live = detail ? detail.status !== "exited" && detail.status !== "error" : false;
  const working = turnInFlight(detail);
  const typed = draft.trim().length > 0;
  const available = availableProviderIds(registered, providerStatuses);
  const modelsByProvider = useMemo(
    () => Object.fromEntries(registered.map((spec) => [spec.id, spec.models])),
    [registered],
  );
  const modelGroups = modelGroupsFor(prefs.enabledProviders, available, modelsByProvider);
  const fallbackProvider = firstAvailableProvider(prefs.enabledProviders, available, prefs.defaultProvider);
  const currentModel = model ?? defaultModelFor(detail?.provider ?? fallbackProvider);
  const known = modelGroups.some((group) => group.models.includes(currentModel));

  useEffect(() => {
    const node = inputRef.current;
    if (!node) return;
    node.style.height = "auto";
    node.style.height = `${Math.min(node.scrollHeight, 132)}px`;
  }, [draft]);

  const submit = async () => {
    if (!typed) return;
    if (working) {
      enqueue(sessionId, draft);
      setDraft(sessionId, "");
      return;
    }
    if (!live) return;
    if (await send(bridge, draft)) setDraft(sessionId, "");
  };

  const changeModel = async (value: string) => {
    if (!detail || value === currentModel) return;
    const next = modelGroups.find((group) => group.models.includes(value))?.provider.id;
    if (!next) return;
    try {
      await switchProvider(bridge, next, value);
      notify(`Session now runs on ${providerMeta(next).name} · ${value}`);
    } catch {
      // The store already carries the error message.
    }
  };

  const isStopped = !live;

  return (
    <div className="composer">
      <div className="composer-inner">
        {pinned.length ? (
          <div className="context-row">
            {pinned.map((pin) => (
              <span className="chip" key={pin.specId}>
                <span className="c-kind">spec</span>
                <span className="c-name">{pin.title}</span>
                <button
                  type="button"
                  aria-label={`Remove ${pin.title}`}
                  onClick={() => onUnpin(pin.specId)}
                >
                  <Icon name="x" size="xs" />
                </button>
              </span>
            ))}
          </div>
        ) : null}
        {queue.length ? (
          <div className="queue-row">
            {queue.map((message, index) => (
              <span className="queue-chip" key={`${message}-${index}`}>
                <span className="q-kind">queued</span>
                <span className="q-text">{message}</span>
                <button
                  type="button"
                  aria-label="Remove queued message"
                  onClick={() => dequeue(sessionId, index)}
                >
                  <Icon name="x" size="xs" />
                </button>
              </span>
            ))}
          </div>
        ) : null}
        <div className="composer-box">
          <label className="sr-only" htmlFor="composer-input">
            Message the active session
          </label>
          <textarea
            id="composer-input"
            ref={inputRef}
            rows={1}
            placeholder={
              isStopped
                ? "Type a message, then resume this session to send…"
                : "Ask for the next change…"
            }
            value={draft}
            disabled={!detail}
            onChange={(event) => setDraft(sessionId, event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                void submit();
              }
            }}
          />
          <div className="composer-foot">
            <label className="sr-only" htmlFor="model-select">
              Model for this session
            </label>
            <select
              className="model-select"
              id="model-select"
              value={currentModel}
              disabled={busy}
              title={isStopped ? "Pick an available model to resume this session" : undefined}
              onChange={(event) => void changeModel(event.target.value)}
            >
              {!known ? <option value={currentModel}>{currentModel}</option> : null}
              {modelGroups.map(({ provider: entry, models }) => (
                <optgroup label={entry.name} key={entry.id}>
                  {models.map((value) => (
                    <option key={value} value={value}>
                      {value}
                    </option>
                  ))}
                </optgroup>
              ))}
            </select>
            <div className="foot-actions">
              {working ? (
                <button className="btn-stop" type="button" onClick={() => void cancel(bridge)}>
                  <Icon name="stop" size="sm" />
                  Stop
                </button>
              ) : null}
              {isStopped ? (
                <button
                  className="btn-ghost sm"
                  type="button"
                  disabled={busy}
                  aria-label="Resume session"
                  onClick={() => void resume(bridge)}
                >
                  Resume
                </button>
              ) : null}
              <button
                className="btn-send"
                type="button"
                disabled={!typed || isStopped}
                onClick={() => void submit()}
                aria-label={working ? "Queue this message for the next turn" : "Send message"}
              >
                <Icon name={working && typed ? "arrowRight" : "send"} size="sm" />
                {working && typed ? "Queue" : "Send"}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
