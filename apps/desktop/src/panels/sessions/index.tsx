import { useVirtualizer } from "@tanstack/react-virtual";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
} from "react";
import { useBridge } from "../../ipc/context";
import { subscribeToHarnessEvents } from "../../ipc/events";
import { PanelFrame } from "../PanelFrame";
import { useSessionStore } from "../../state/sessionStore";
import { foldEvents, type TranscriptItem } from "./transcript";
import "./sessions.css";

/**
 * The Sessions panel: the session list, a harness picker limited to what the core registered, and the
 * open conversation.
 *
 * Several sessions stay open side by side as tabs. Each tab keeps its own transcript, compose text and
 * status; only the active tab's event list is rendered, because the log is the record and a background
 * session has nothing to paint.
 *
 * The header is the session's knowledge: its harness (switchable, which re-launches the same
 * conversation on another harness), the specs pinned to it, and the plan — plus the one action that
 * moves a snapshot, "Refresh context".
 *
 * Rendering is driven by the persisted event log, so a reload rebuilds the same transcript; live
 * events arrive over `grove://session-event`.
 */
export function SessionsPanel() {
  const bridge = useBridge();
  const harnesses = useSessionStore((state) => state.harnesses);
  const sessions = useSessionStore((state) => state.sessions);
  const detail = useSessionStore((state) => state.detail);
  const events = useSessionStore((state) => state.events);
  const context = useSessionStore((state) => state.context);
  const openIds = useSessionStore((state) => state.openIds);
  const drafts = useSessionStore((state) => state.drafts);
  const activeId = useSessionStore((state) => state.activeId);
  const loading = useSessionStore((state) => state.loading);
  const busy = useSessionStore((state) => state.busy);
  const error = useSessionStore((state) => state.error);
  const load = useSessionStore((state) => state.load);
  const create = useSessionStore((state) => state.create);
  const select = useSessionStore((state) => state.select);
  const closeTab = useSessionStore((state) => state.closeTab);
  const setDraft = useSessionStore((state) => state.setDraft);
  const send = useSessionStore((state) => state.send);
  const cancel = useSessionStore((state) => state.cancel);
  const resume = useSessionStore((state) => state.resume);
  const close = useSessionStore((state) => state.close);
  const refreshContext = useSessionStore((state) => state.refreshContext);
  const switchHarness = useSessionStore((state) => state.switchHarness);

  const [harness, setHarness] = useState("");
  const [cwd, setCwd] = useState("");

  useEffect(() => {
    void load(bridge);
  }, [bridge, load]);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void subscribeToHarnessEvents(bridge, (sessionId, event) => {
      useSessionStore.getState().applyEvent(sessionId, event);
    }).then((stop) => {
      if (active) unlisten = stop;
      else stop();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }, [bridge]);

  useEffect(() => {
    if (!harness && harnesses.length > 0) setHarness(harnesses[0].id);
  }, [harness, harnesses]);

  const items = useMemo(() => foldEvents(events), [events]);
  const status = detail?.status ?? null;
  const running = status !== null && status !== "exited" && status !== "error";
  const draft = activeId ? (drafts[activeId] ?? "") : "";

  const submitCreate = async (formEvent: FormEvent) => {
    formEvent.preventDefault();
    if (!harness || busy) return;
    await create(bridge, { harness, cwd: cwd.trim() || null });
  };

  const submitMessage = async (formEvent: FormEvent) => {
    formEvent.preventDefault();
    const id = activeId;
    if (!id || !draft.trim()) return;
    if (await send(bridge, draft)) setDraft(id, "");
  };

  return (
    <PanelFrame
      title="Sessions"
      subtitle="One conversation per harness, stored locally and resumable after a restart."
    >
      <div className="sessions" data-testid="sessions-body">
        <aside className="session-list" data-testid="session-list">
          <form className="session-create" onSubmit={submitCreate}>
            <label className="field">
              <span>Harness</span>
              <select
                className="input"
                aria-label="Harness"
                data-testid="harness-picker"
                value={harness}
                onChange={(event) => setHarness(event.target.value)}
              >
                {harnesses.map((info) => (
                  <option key={info.id} value={info.id}>
                    {info.id}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>Project directory</span>
              <input
                className="input"
                aria-label="Project directory"
                placeholder="Defaults to the app directory"
                value={cwd}
                onChange={(event) => setCwd(event.target.value)}
              />
            </label>
            <button
              type="submit"
              className="input primary"
              data-testid="session-create"
              disabled={busy || !harness}
            >
              Create session
            </button>
          </form>

          {loading ? <p className="session-empty">Loading sessions…</p> : null}
          {!loading && sessions.length === 0 ? (
            <p className="session-empty">No sessions yet. Pick a harness and create one.</p>
          ) : null}

          <ul className="session-items">
            {sessions.map((row) => {
              const active = row.id === detail?.id;
              return (
                <li key={row.id}>
                  <button
                    type="button"
                    className={active ? "session-item is-active" : "session-item"}
                    aria-current={active ? "true" : undefined}
                    data-testid={`session-item-${row.id}`}
                    onClick={() => void select(bridge, row.id)}
                  >
                    <span className={`status-dot status-${row.status}`} aria-hidden="true" />
                    <span className="session-title">{row.title}</span>
                    <span className="session-meta">
                      {row.harness} · {row.status} · {row.eventCount} events
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        </aside>

        <section className="session-main" data-testid="session-main">
          {openIds.length > 0 ? (
            <div className="session-tabs" role="tablist" aria-label="Open sessions" data-testid="session-tabs">
              {openIds.map((id) => {
                const row = sessions.find((session) => session.id === id);
                const active = id === activeId;
                return (
                  <div key={id} className={active ? "session-tab is-active" : "session-tab"}>
                    <button
                      type="button"
                      role="tab"
                      aria-selected={active}
                      className="session-tab-label"
                      data-testid={`session-tab-${id}`}
                      onClick={() => void select(bridge, id)}
                    >
                      <span
                        className={`status-dot status-${row?.status ?? detail?.status ?? "idle"}`}
                        aria-hidden="true"
                      />
                      {row?.title ?? detail?.title ?? id}
                    </button>
                    <button
                      type="button"
                      className="session-tab-close"
                      aria-label={`Close tab ${row?.title ?? id}`}
                      data-testid={`session-tab-close-${id}`}
                      onClick={() => void closeTab(bridge, id)}
                    >
                      ×
                    </button>
                  </div>
                );
              })}
            </div>
          ) : null}

          {detail ? (
            <>
              <header className="session-header">
                <div>
                  <h2 className="session-heading">{detail.title}</h2>
                  <p className="session-meta">
                    {detail.harness} · {detail.cwd} · {detail.status}
                  </p>
                </div>
                <div className="session-actions">
                  <label className="field session-switch">
                    <span>Harness</span>
                    <select
                      className="input"
                      aria-label="Session harness"
                      data-testid="session-harness"
                      value={detail.harness}
                      disabled={busy}
                      onChange={(event) => void switchHarness(bridge, event.target.value)}
                    >
                      {harnesses.map((info) => (
                        <option key={info.id} value={info.id}>
                          {info.id}
                        </option>
                      ))}
                    </select>
                  </label>
                  <button
                    type="button"
                    className="input"
                    data-testid="session-refresh-context"
                    disabled={!running || busy}
                    onClick={() => void refreshContext(bridge)}
                  >
                    Refresh context
                  </button>
                  {running ? (
                    <button
                      type="button"
                      className="input"
                      data-testid="session-close"
                      disabled={busy}
                      onClick={() => void close(bridge)}
                    >
                      Close
                    </button>
                  ) : (
                    <button
                      type="button"
                      className="input primary"
                      data-testid="session-resume"
                      disabled={busy}
                      onClick={() => void resume(bridge)}
                    >
                      Resume
                    </button>
                  )}
                </div>
              </header>

              <section className="session-context" aria-label="Session context" data-testid="session-context">
                <span className="session-context-label">Context</span>
                {context && context.pins.length > 0 ? (
                  context.pins.map((pin) => (
                    <span key={pin.specId} className="session-context-pin" data-testid="session-context-pin">
                      {pin.title}
                    </span>
                  ))
                ) : (
                  <span className="session-context-empty" data-testid="session-context-empty">
                    No specs pinned
                  </span>
                )}
                <span className="session-context-plan" data-testid="session-context-plan">
                  {context && context.planVersion > 0 ? `plan v${context.planVersion}` : "no plan"}
                </span>
                <span className="session-context-size" data-testid="session-context-size">
                  {context ? `${context.documents.length} docs · ${context.totalChars} chars` : "not assembled"}
                </span>
              </section>

              <Conversation items={items} />
              {items.length === 0 ? (
                <p className="session-empty" data-testid="session-empty">
                  No messages yet. Say something to start the conversation.
                </p>
              ) : null}

              <form className="compose" onSubmit={submitMessage}>
                <textarea
                  className="input"
                  aria-label="Message"
                  data-testid="session-compose"
                  placeholder={`Message ${detail.harness}…`}
                  rows={2}
                  value={draft}
                  onChange={(event) => setDraft(detail.id, event.target.value)}
                />
                <div className="compose-actions">
                  <button
                    type="submit"
                    className="input primary"
                    data-testid="session-send"
                    disabled={!running || !draft.trim()}
                  >
                    Send
                  </button>
                  <button
                    type="button"
                    className="input"
                    data-testid="session-cancel"
                    disabled={!running}
                    onClick={() => void cancel(bridge)}
                  >
                    Cancel
                  </button>
                </div>
              </form>
            </>
          ) : (
            <div className="session-blank" data-testid="session-blank">
              <p>Select a session, or create one to start a conversation.</p>
            </div>
          )}

          {error ? (
            <p className="session-error" role="alert" data-testid="session-error">
              {error}
            </p>
          ) : null}
        </section>
      </div>
    </PanelFrame>
  );
}

/** The virtualized conversation. Only the rows in view are mounted, so long transcripts stay cheap. */
function Conversation({ items }: { items: TranscriptItem[] }) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 96,
    overscan: 6,
    // jsdom has no layout engine; a sane initial box means tests render rows before the observer
    // reports a real one, exactly like the first paint in the browser.
    initialRect: { width: 0, height: 640 },
  });

  return (
    <div className="conversation" ref={scrollRef} data-testid="session-conversation">
      <div className="conversation-inner" style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((row) => (
          <div
            key={row.key}
            className="conversation-row"
            style={{ transform: `translateY(${row.start}px)` }}
          >
            <TranscriptRow item={items[row.index]} />
          </div>
        ))}
      </div>
    </div>
  );
}

function TranscriptRow({ item }: { item: TranscriptItem }) {
  switch (item.kind) {
    case "message":
      return (
        <article className={`bubble bubble-${item.role}`} data-testid="transcript-message">
          <span className="bubble-role">{item.role}</span>
          {item.blocks.map((block, index) =>
            block.type === "code" ? (
              <pre key={index} className="bubble-code">
                {block.text}
              </pre>
            ) : (
              <p key={index} className={block.type === "thinking" ? "bubble-thinking" : undefined}>
                {block.text}
              </p>
            ),
          )}
        </article>
      );
    case "stream":
      return (
        <p className="bubble bubble-assistant is-streaming" data-testid="transcript-stream">
          {item.text}
        </p>
      );
    case "tool":
      return (
        <article className="tool-card" data-testid="transcript-tool-call">
          <header className="tool-head">
            <span className="tool-name">{item.name}</span>
            <span className={`tool-status tool-${item.status ?? "pending"}`}>
              {item.status ?? "running"}
            </span>
          </header>
          <p className="tool-title">{item.title}</p>
          {item.summary ? <p className="tool-summary">{item.summary}</p> : null}
        </article>
      );
    case "diff":
      return (
        <article className="tool-card" data-testid="transcript-diff">
          <header className="tool-head">
            <span className="tool-name">{item.path}</span>
            <span className="tool-status">
              +{item.added} −{item.removed}
            </span>
          </header>
          <pre className="bubble-code">{item.patch}</pre>
        </article>
      );
    case "notice":
      return (
        <p
          className={item.tone === "error" ? "notice is-error" : "notice"}
          data-testid="transcript-notice"
          role={item.tone === "error" ? "alert" : undefined}
        >
          {item.text}
        </p>
      );
    case "usage":
      return (
        <p className="transcript-meta" data-testid="transcript-usage">
          {item.inputTokens} tokens in · {item.outputTokens} out
        </p>
      );
  }
}
