import type {
  Decision,
  Project,
  QuestionAnswer,
  QuestionChoice,
  SessionSummary,
  ThinkingLevel,
} from "@grove/contract";
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";

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
  knownThinkingLevels,
  nearestThinkingLevel,
  thinkingGroups,
  thinkingLevelsFor,
} from "../workspace/thinking";
import { Combo } from "../shell/Combo";
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
  const respondPermission = useSessionStore((state) => state.respondPermission);
  const respondQuestion = useSessionStore((state) => state.respondQuestion);
  const error = useSessionStore((state) => state.error);
  const applyContext = useSessionStore((state) => state.applyContext);
  const notify = useUiStore((state) => state.notify);
  const openNewSession = useUiStore((state) => state.openNewSession);

  const items = useMemo(() => foldTranscript(events), [events]);
  const questionPending = useMemo(
    () => items.some((item) => item.kind === "question" && item.answers === null),
    [items],
  );
  const working = turnInFlight(summary ?? detail);
  const visual = sessionVisual(summary ?? detail);
  const provider = detail ? providerMeta(detail.provider) : null;

  const headerActions = (
    <div className="sh-right">
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
        onAnswer={(requestId, answers) => respondQuestion(bridge, requestId, answers)}
        empty={!detail}
        onNewSession={() => openNewSession({ projectId: project?.id ?? null })}
      />

      {detail ? (
        <Composer
          sessionId={detail.id}
          model={detail.model}
          locked={questionPending}
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
  onAnswer,
  empty,
  onNewSession,
}: {
  items: TranscriptItem[];
  working: boolean;
  providerName: string;
  onDecide: (requestId: string, decision: Decision) => void;
  onAnswer: (requestId: string, answers: QuestionAnswer[]) => Promise<boolean>;
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
                    onAnswer={onAnswer}
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

function AssistantFoot({
  provider,
  at,
  durationMs,
  streaming = false,
}: {
  provider: string;
  at: string;
  durationMs?: number | null;
  streaming?: boolean;
}) {
  const timing = durationLabel(durationMs ?? null) || (streaming ? "" : clockLabel(at));
  return (
    <div className="turn-foot">
      <span>{providerMeta(provider).name}</span>
      {timing ? (
        <>
          <span className="sep" aria-hidden="true">
            ·
          </span>
          <span>{timing}</span>
        </>
      ) : null}
    </div>
  );
}

function TranscriptRow({
  item,
  open,
  onToggle,
  onDecide,
  onAnswer,
}: {
  item: TranscriptItem;
  open: boolean;
  onToggle: () => void;
  onDecide: (requestId: string, decision: Decision) => void;
  onAnswer: (requestId: string, answers: QuestionAnswer[]) => Promise<boolean>;
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
          <AssistantFoot provider={provider} at={item.at} durationMs={item.durationMs} />
        </div>
      );
    }
    case "stream":
      return (
        <div className="turn">
          <p>
            {item.text}
            <span className="stream-caret" aria-hidden="true" />
          </p>
          <AssistantFoot provider={provider} at={item.at} streaming />
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
    case "question":
      return <QuestionCard item={item} onAnswer={onAnswer} />;
    case "usage":
      return null;
  }
}

type QuestionItem = Extract<TranscriptItem, { kind: "question" }>;

function answerSummary(answers: QuestionAnswer[], questionId: string): string {
  const answer = answers.find((entry) => entry.question_id === questionId);
  if (!answer || answer.answers.length === 0) return "";
  return answer.answers
    .map((choice) => (choice.note ? `${choice.label} — ${choice.note}` : choice.label))
    .join(", ");
}

function ResolvedQuestionCard({ item }: { item: QuestionItem }) {
  const answers = item.answers ?? [];
  return (
    <div className="turn">
      <div className="qa-list" role="group" aria-label="Answered questions">
        {item.questions.map((question) => {
          const line = answerSummary(answers, question.id);
          return (
            <div className="qa" key={question.id}>
              <span className="qk">{line ? "answered" : "closed"}</span>
              <span className="qv">
                <b>{question.title}</b>
                {line ? ` — ${line}` : " — no answer was sent."}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** An inline question card: one question at a time, all answers sent together. */
function QuestionCard({
  item,
  onAnswer,
}: {
  item: QuestionItem;
  onAnswer: (requestId: string, answers: QuestionAnswer[]) => Promise<boolean>;
}) {
  const detail = useSessionStore((state) => state.detail);
  const live = detail ? detail.status !== "exited" && detail.status !== "error" : false;
  const [index, setIndex] = useState(0);
  const [answers, setAnswers] = useState<Record<string, QuestionChoice[]>>({});
  const [other, setOther] = useState<Record<string, string>>({});
  const [otherOpen, setOtherOpen] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const cardRef = useRef<HTMLDivElement>(null);

  const resolved = item.answers !== null;
  const total = item.questions.length;
  const current = item.questions[Math.min(index, total - 1)];
  const selected = current ? (answers[current.id] ?? []) : [];
  const currentOther = current ? (other[current.id] ?? "") : "";
  const isSelected = (label: string) => selected.some((choice) => choice.label === label);

  const answered = (question: (typeof item.questions)[number]) =>
    (answers[question.id]?.length ?? 0) > 0 || Boolean(other[question.id]?.trim());
  const currentAnswered = current ? answered(current) : false;
  const allAnswered = item.questions.every(answered);

  useEffect(() => {
    if (!resolved && !dismissed) cardRef.current?.focus();
  }, [dismissed, resolved]);

  useEffect(() => {
    if (index >= total && total > 0) setIndex(total - 1);
  }, [index, total]);

  if (resolved) return <ResolvedQuestionCard item={item} />;
  if (!current) return null;
  if (!live) {
    return (
      <div className="turn">
        <div className="qcard qcard-closed" role="group" aria-label={current.title}>
          <div className="qhead">
            <span className="qkind">
              <Icon name="alert" size="sm" /> question
            </span>
            <span className="qstep">closed</span>
          </div>
          <p className="qtitle">{current.title}</p>
          <p className="qhint">The session stopped before this question was answered.</p>
        </div>
      </div>
    );
  }

  const select = (label: string) => {
    if (!current) return;
    const existing = answers[current.id] ?? [];
    if (current.multi) {
      const next = isSelected(label)
        ? existing.filter((choice) => choice.label !== label)
        : [...existing, { label }];
      setAnswers((state) => ({ ...state, [current.id]: next }));
    } else {
      setAnswers((state) => ({ ...state, [current.id]: [{ label }] }));
    }
  };

  const setNote = (label: string, note: string) => {
    if (!current) return;
    const trimmed = note.trim();
    setAnswers((state) => ({
      ...state,
      [current.id]: (state[current.id] ?? []).map((choice) =>
        choice.label === label ? { ...choice, note: trimmed || undefined } : choice,
      ),
    }));
  };

  const next = () => {
    if (index < total - 1) {
      setIndex((value) => value + 1);
      setOtherOpen(false);
    }
  };

  const submit = async () => {
    if (!allAnswered || submitting) return;
    setSubmitting(true);
    const payload: QuestionAnswer[] = item.questions.map((question) => {
      const typed = other[question.id]?.trim();
      let choices = [...(answers[question.id] ?? [])];
      if (typed) {
        const freeform = { label: `Other: ${typed}` };
        choices = question.multi ? [...choices, freeform] : [freeform];
      }
      return { question_id: question.id, answers: choices };
    });
    const accepted = await onAnswer(item.requestId, payload);
    if (!accepted) setSubmitting(false);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) {
      if (event.key === "Escape") {
        event.preventDefault();
        setDismissed(true);
      }
      return;
    }
    if (/^[1-9]$/.test(event.key)) {
      const option = current.options[Number(event.key) - 1];
      if (option) {
        event.preventDefault();
        select(option.label);
      }
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      setDismissed(true);
      return;
    }
    if (event.key === "Enter" && !(event.target instanceof HTMLButtonElement)) {
      event.preventDefault();
      if (!currentAnswered) return;
      if (index === total - 1) void submit();
      else next();
    }
  };

  if (dismissed) {
    return (
      <div className="turn">
        <div className="dismissed-card">
          <Icon name="alert" size="sm" />
          <span>Question dismissed — the agent is paused.</span>
          <button className="btn-ghost sm" type="button" onClick={() => setDismissed(false)}>
            Answer it
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="turn">
      <div
        className="qcard"
        role="group"
        aria-label={current.title}
        ref={cardRef}
        tabIndex={-1}
        onKeyDown={onKeyDown}
      >
        <div className="qhead">
          <span className="qkind">
            <Icon name="alert" size="sm" /> question
          </span>
          <span className="qstep">
            {index + 1} of {total}
          </span>
        </div>
        <p className="qtitle">{current.title}</p>
        {current.hint ? <p className="qhint">{current.hint}</p> : null}
        <div className="qopts">
          {current.options.map((option) => {
            const on = isSelected(option.label);
            const note = selected.find((choice) => choice.label === option.label)?.note ?? "";
            return (
              <div className="qwrap" key={option.label}>
                <button
                  className="qopt"
                  type="button"
                  role={current.multi ? "checkbox" : "radio"}
                  aria-checked={on}
                  onClick={() => select(option.label)}
                >
                  <span className="box" aria-hidden="true">
                    {on ? <Icon name="check" size="sm" /> : null}
                  </span>
                  <span className="grow">
                    <span className="t">{option.label}</span>
                    {option.description ? <span className="s">{option.description}</span> : null}
                  </span>
                </button>
                {on ? (
                  <div className="qnote">
                    <span className="ico" aria-hidden="true">
                      <Icon name="message" size="sm" />
                    </span>
                    <label className="sr-only" htmlFor={`note-${item.id}-${option.label}`}>
                      Add a note to {option.label}
                    </label>
                    <input
                      id={`note-${item.id}-${option.label}`}
                      type="text"
                      value={note}
                      placeholder="Add a note to this answer — optional"
                      autoComplete="off"
                      onChange={(event) => setNote(option.label, event.target.value)}
                    />
                  </div>
                ) : null}
              </div>
            );
          })}
        </div>
        <button className="other-link" type="button" onClick={() => setOtherOpen((open) => !open)}>
          Other — type your own answer
        </button>
        {otherOpen ? (
          <div className="other-row">
            <input
              type="text"
              value={currentOther}
              placeholder="Type your answer…"
              autoComplete="off"
              aria-label={`Other answer for ${current.title}`}
              onChange={(event) =>
                setOther((state) => ({ ...state, [current.id]: event.target.value }))
              }
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  setOtherOpen(false);
                }
              }}
            />
            <button
              className="btn-ghost sm"
              type="button"
              disabled={!currentOther.trim()}
              onClick={() => setOtherOpen(false)}
            >
              Use
            </button>
          </div>
        ) : null}
        <p className="note-hint">
          <Icon name="message" size="sm" /> Any answer can carry a note — it goes back to the agent
          with the choice.
        </p>
        <div className="qfoot">
          <span className="qkeys">
            <span className="kbd">1</span>–<span className="kbd">{current.options.length}</span>
            <b>choose</b>
            <span className="kbd">↵</span>
            <b>{index === total - 1 ? "send" : "next"}</b>
          </span>
          {index > 0 ? (
            <button
              className="btn-ghost sm qback"
              type="button"
              onClick={() => setIndex((value) => Math.max(0, value - 1))}
            >
              Back
            </button>
          ) : null}
          <button
            className="btn-primary sm"
            type="button"
            disabled={submitting || !currentAnswered}
            onClick={() => (index === total - 1 ? void submit() : next())}
          >
            {submitting
              ? "Sending…"
              : index === total - 1
                ? allAnswered
                  ? "Send answer"
                  : "Answer all questions"
                : "Next question"}
          </button>
        </div>
      </div>
    </div>
  );
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
  locked,
  pinned,
  onUnpin,
}: {
  sessionId: string;
  model: string | null;
  locked: boolean;
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
  const setThinking = useSessionStore((state) => state.setThinking);
  const busy = useSessionStore((state) => state.busy);
  const prefs = useWorkspaceStore((state) => state.prefs);
  const registered = useSessionStore((state) => state.providers);
  const providerStatuses = useSessionStore((state) => state.providerStatuses);
  const modelThinking = useSessionStore((state) => state.modelThinking);
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
  const currentProvider = detail?.provider ?? fallbackProvider;
  const currentModel = model ?? defaultModelFor(currentProvider, modelsByProvider[currentProvider]);
  const currentThinking = detail?.thinkingLevel ?? "off";
  const knownLevels = useMemo(
    () => knownThinkingLevels(currentProvider, currentModel, modelThinking),
    [currentModel, currentProvider, modelThinking],
  );
  const thinkingLevels = knownLevels ?? thinkingLevelsFor(currentProvider, currentModel, modelThinking);

  useEffect(() => {
    const node = inputRef.current;
    if (!node) return;
    node.style.height = "auto";
    node.style.height = `${Math.min(node.scrollHeight, 132)}px`;
  }, [draft]);

  // A saved level can outlive the model it was chosen for. As soon as the capability catalogue
  // knows the current model, move an unsupported level to the nearest one it does support. This
  // also handles an old session whose `off` is not an option on a mandatory-reasoning model.
  useEffect(() => {
    if (!detail || !knownLevels || knownLevels.includes(currentThinking)) return;
    void setThinking(bridge, nearestThinkingLevel(currentThinking, knownLevels));
  }, [bridge, currentThinking, detail?.id, knownLevels, setThinking]);

  const submit = async () => {
    if (!typed || locked) return;
    if (working) {
      enqueue(sessionId, draft);
      setDraft(sessionId, "");
      return;
    }
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
              locked
                ? "Answer the question in the transcript to continue…"
                : isStopped
                  ? "Continue where this session left off…"
                  : "Ask for the next change…"
            }
            value={draft}
            disabled={!detail || locked}
            onChange={(event) => setDraft(sessionId, event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                void submit();
              }
            }}
          />
          <div className="composer-foot">
            <Combo
              className="combo-model"
              id="model-select"
              label="Model for this session"
              value={currentModel}
              groups={modelGroups.map(({ provider: entry, models }) => ({
                label: entry.name,
                options: models.map((value) => ({ value })),
              }))}
              disabled={busy}
              searchable
              placement="up"
              onChange={(value) => void changeModel(value)}
            />
            <Combo
              className="combo-thinking"
              id="thinking-select"
              label="Thinking level for this session"
              value={currentThinking}
              groups={thinkingGroups(thinkingLevels)}
              disabled={busy}
              placement="up"
              title="Thinking level for the next turn"
              onChange={(value) => void setThinking(bridge, value as ThinkingLevel)}
            />
            <div className="foot-actions">
              {working ? (
                <button className="btn-stop" type="button" onClick={() => void cancel(bridge)}>
                  <Icon name="stop" size="sm" />
                  Stop
                </button>
              ) : null}
              <button
                className="btn-send"
                type="button"
                disabled={!typed || locked}
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
