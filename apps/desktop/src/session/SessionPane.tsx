import type {
  Block,
  Decision,
  Project,
  QuestionAnswer,
  QuestionChoice,
  SessionSummary,
  ThinkingLevel,
} from "@grove/contract";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from "react";

const EMPTY_QUEUE: string[] = [];


import { useVirtualizer } from "@tanstack/react-virtual";
import { useBridge } from "../ipc/context";
import { errorText } from "../ipc/errors";
import { useSessionStore } from "../state/sessionStore";
import type { MenuItem } from "../state/menuStore";
import { useMenuStore } from "../state/menuStore";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore } from "../state/workspaceStore";
import {
  availableProviderIds,
  defaultModelFor,
  firstAvailableProvider,
  providerMeta,
  turnInFlight,
  sessionVisual,
  SESSION_UI_STATE,
  PROVIDERS,
  type ProviderMeta,
} from "../workspace/models";
import { Icon, type IconName } from "../workspace/Icon";
import { FileTypeIcon } from "../workspace/FileTypeIcon";
import { ProviderGlyph } from "../workspace/ProviderGlyph";
import { modelLabel } from "../workspace/modelLabel";
import {
  knownThinkingLevels,
  nearestThinkingLevel,
  thinkingGroups,
  thinkingLabel,
  thinkingLevelsFor,
} from "../workspace/thinking";
import { Combo, type ComboGroup, type ComboOption } from "../shell/Combo";
import {
  clockLabel,
  durationLabel,
  foldTranscript,
  footerIndexes,
  toolTarget,
  toolTiming,
  type TranscriptItem,
} from "../session/transcript";
import { copyText } from "../shell/clipboard";
import { revealPath } from "../shell/reveal";

const RECENT_MODELS_KEY = "grove.recentModels";
const RECENT_LIMIT = 5;

/** One model and the provider it runs through, as the picker menus see it. */
interface ModelChoice {
  id: string;
  meta: ProviderMeta;
  ready: boolean;
}

function readRecentModels(): string[] {
  try {
    const raw = localStorage.getItem(RECENT_MODELS_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed)
      ? parsed.filter((entry): entry is string => typeof entry === "string")
      : [];
  } catch {
    // Storage can be blocked or hold an older value; recents are a convenience, not state.
    return [];
  }
}

function writeRecentModels(models: string[]): void {
  try {
    localStorage.setItem(RECENT_MODELS_KEY, JSON.stringify(models));
  } catch {
    // Same: the picker still works without remembering anything.
  }
}

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
  const restoreSession = useSessionStore((state) => state.restore);
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
      {detail?.archivedAt ? (
        <button
          className="btn-ghost sm"
          type="button"
          aria-label="Restore session"
          title="Move this session back to the active tree"
          onClick={() => void restoreSession(bridge, detail.id)}
        >
          Restore
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
            {detail.model ? (
              <span className="sh-model" title={detail.model}>
                {modelLabel(detail.model, detail.provider)}
              </span>
            ) : null}
            {detail.archivedAt ? (
              <span className="sh-pill archived" title="This session is archived">
                Archived
              </span>
            ) : null}
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
        footer={
          detail ? (
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
          ) : null
        }
      />

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
  footer,
}: {
  items: TranscriptItem[];
  working: boolean;
  providerName: string;
  onDecide: (requestId: string, decision: Decision) => void;
  onAnswer: (requestId: string, answers: QuestionAnswer[]) => Promise<boolean>;
  empty: boolean;
  onNewSession: () => void;
  /** The composer, kept inside the scrollport so the scrollbar runs to the bottom of the chat. */
  footer?: ReactNode;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  const scrollTimer = useRef<number | undefined>(undefined);
  const [scrolling, setScrolling] = useState(false);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const footers = useMemo(() => footerIndexes(items, working), [items, working]);
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
    // The scrollbar stays out of sight until the transcript actually moves.
    setScrolling(true);
    window.clearTimeout(scrollTimer.current);
    scrollTimer.current = window.setTimeout(() => setScrolling(false), 900);
  };

  useEffect(() => () => window.clearTimeout(scrollTimer.current), []);

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
      data-scrolling={scrolling ? "true" : undefined}
      tabIndex={0}
      role="region"
      aria-label="Session transcript and composer"
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
                    footer={footers.has(row.index)}
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
      </div>
      {footer}
    </div>
  );
}

function AssistantFoot({
  provider,
  at,
  durationMs,
}: {
  provider: string;
  at: string;
  durationMs?: number | null;
}) {
  const timing = durationLabel(durationMs ?? null) || clockLabel(at);
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

function fullText(blocks: Block[]): string {
  return blocks.map((block) => block.text).join("\n");
}

function markdownOf(blocks: Block[]): string {
  return blocks
    .map((block) =>
      block.type === "code" ? `\`\`\`${block.language}\n${block.text}\n\`\`\`` : block.text,
    )
    .join("\n\n");
}

function codeOf(blocks: Block[]): string | null {
  const code = blocks.filter(
    (block): block is Extract<Block, { type: "code" }> => block.type === "code",
  );
  return code.length > 0 ? code.map((block) => block.text).join("\n\n") : null;
}

/**
 * The file a tool row points at, if its target looks like one. Absolute paths stand alone; a relative
 * one is resolved against the session's cwd so Reveal has a real path to hand the core.
 */
function resolveToolPath(title: string, cwd: string | null): string | null {
  const raw = title.trim();
  if (!raw) return null;
  if (/^(?:[a-zA-Z]:[\\/]|[/\\]|~[\\/])/.test(raw)) return raw;
  if (!/[\\/]/.test(raw) && !/\.[a-z0-9]{1,8}$/i.test(raw)) return null;
  if (!cwd) return null;
  return `${cwd.replace(/[\\/]+$/, "")}/${raw.replace(/\\/g, "/")}`;
}

/** The action mark for a tool row; a file-type mark sits beside the target for file tools. */
function toolIcon(name: string): IconName {
  switch (name) {
    case "read_file":
      return "file";
    case "list_dir":
      return "folder";
    case "grep":
      return "search";
    case "write_file":
    case "edit_file":
      return "edit";
    case "run_command":
      return "terminal";
    case "ask_user":
      return "message";
    case "subagent":
      return "layers";
    default:
      return "terminal";
  }
}

function openAt(event: ReactMouseEvent<HTMLElement>, items: MenuItem[]) {
  event.preventDefault();
  useMenuStore.getState().openMenu({
    x: event.clientX,
    y: event.clientY,
    returnFocus: event.currentTarget,
    items,
  });
}

export function TranscriptRow({
  item,
  footer,
  open,
  onToggle,
  onDecide,
  onAnswer,
}: {
  item: TranscriptItem;
  footer: boolean;
  open: boolean;
  onToggle: () => void;
  onDecide: (requestId: string, decision: Decision) => void;
  onAnswer: (requestId: string, answers: QuestionAnswer[]) => Promise<boolean>;
}) {
  const bridge = useBridge();
  const [answered, setAnswered] = useState<Decision | null>(null);
  const detail = useSessionStore((state) => state.detail);
  const provider = detail?.provider ?? "mock";
  const notify = useUiStore((state) => state.notify);

  const copy = (text: string) => {
    void copyText(text).then((ok) => {
      if (!ok) notify("Clipboard is unavailable", "error");
    });
  };

  const openMessageMenu = (event: ReactMouseEvent<HTMLElement>, blocks: Block[]) => {
    const text = fullText(blocks);
    const items: MenuItem[] = [
      { kind: "item", label: "Copy", icon: "copy", onSelect: () => copy(text) },
      {
        kind: "item",
        label: "Copy as Markdown",
        icon: "file",
        onSelect: () => copy(markdownOf(blocks)),
      },
    ];
    const code = codeOf(blocks);
    if (code) {
      items.push({
        kind: "item",
        label: "Copy code",
        icon: "terminal",
        onSelect: () => copy(code),
      });
    }
    openAt(event, items);
  };

  const openTargetMenu = (event: ReactMouseEvent<HTMLElement>, target: string) => {
    const path = resolveToolPath(target, detail?.cwd ?? null);
    const items: MenuItem[] = [];
    if (path) {
      items.push({ kind: "item", label: "Copy path", icon: "copy", onSelect: () => copy(path) });
      items.push({
        kind: "item",
        label: "Reveal in Explorer",
        icon: "folder",
        onSelect: () => void revealPath(bridge, path, notify),
      });
    } else if (target.trim()) {
      items.push({
        kind: "item",
        label: "Copy",
        icon: "copy",
        onSelect: () => copy(target.trim()),
      });
    }
    if (items.length === 0) return;
    openAt(event, items);
  };

  switch (item.kind) {
    case "message": {
      if (item.role === "user") {
        const text = item.blocks
          .filter((block) => block.type === "text")
          .map((block) => block.text)
          .join("\n");
        return (
          <div
            className="turn turn-user"
            onContextMenu={(event) => openMessageMenu(event, item.blocks)}
          >
            <div className="bubble">
              <p>{text}</p>
            </div>
          </div>
        );
      }
      return (
        <div className="turn" onContextMenu={(event) => openMessageMenu(event, item.blocks)}>
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
          {footer ? (
            <AssistantFoot provider={provider} at={item.at} durationMs={item.durationMs} />
          ) : null}
        </div>
      );
    }
    case "stream":
      // Whitespace-only deltas would otherwise leave a bare, blinking row behind tool cards.
      if (!item.text.trim()) return null;
      return (
        <div
          className="turn"
          onContextMenu={(event) =>
            openMessageMenu(event, [{ type: "text", text: item.text }])
          }
        >
          <p>{item.text}</p>
          {footer ? <AssistantFoot provider={provider} at={item.at} /> : null}
        </div>
      );
    case "tool": {
      const target = toolTarget(item.name, item.title);
      const fileTool =
        item.name === "read_file" || item.name === "write_file" || item.name === "edit_file";
      const timing = toolTiming(item.status, item.durationMs);
      return (
        <div className="turn">
          <div
            className="tool"
            data-status={item.status ?? "running"}
            onContextMenu={(event) => openTargetMenu(event, target)}
          >
            <button
              className="tool-head"
              type="button"
              aria-expanded={open}
              onClick={onToggle}
            >
              <span className="tool-ico">
                <Icon name={toolIcon(item.name)} size="sm" />
              </span>
              <span className="tool-name">{item.name}</span>
              {fileTool && target ? (
                <span className="tool-filetype">
                  <FileTypeIcon path={target} />
                </span>
              ) : null}
              <span className="tool-target">{target}</span>
              {timing ? <span className="tool-time">{timing}</span> : null}
              <span className="chev">
                <Icon name="chevron" size="sm" />
              </span>
            </button>
            {open ? (
              <div className="tool-body">
                {target ? <span className="tool-path">{target}</span> : null}
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
          <div
            className="tool"
            data-status="ok"
            onContextMenu={(event) => openTargetMenu(event, item.path)}
          >
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
  const setThinking = useSessionStore((state) => state.setThinking);
  const setPermissionMode = useSessionStore((state) => state.setPermissionMode);
  const setModel = useSessionStore((state) => state.setModel);
  const switchModel = useSessionStore((state) => state.switchModel);
  const pendingModel = useSessionStore((state) => state.pendingModel);
  const busy = useSessionStore((state) => state.busy);
  const prefs = useWorkspaceStore((state) => state.prefs);
  const registered = useSessionStore((state) => state.providers);
  const providerStatuses = useSessionStore((state) => state.providerStatuses);
  const modelThinking = useSessionStore((state) => state.modelThinking);
  const notify = useUiStore((state) => state.notify);
  const openSettings = useUiStore((state) => state.openSettings);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const [recentModels, setRecentModels] = useState<string[]>(readRecentModels);

  const live = detail ? detail.status !== "exited" && detail.status !== "error" : false;
  const working = turnInFlight(detail);
  const typed = draft.trim().length > 0;
  const available = availableProviderIds(registered, providerStatuses);
  const modelsByProvider = useMemo(
    () => Object.fromEntries(registered.map((spec) => [spec.id, spec.models])),
    [registered],
  );
  const fallbackProvider = firstAvailableProvider(prefs.enabledProviders, available, prefs.defaultProvider);
  const currentProvider = detail?.provider ?? fallbackProvider;
  const currentModel = model ?? defaultModelFor(currentProvider, modelsByProvider[currentProvider]);
  const currentThinking = detail?.thinkingLevel ?? "off";
  const permission =
    detail?.permissionMode ?? (prefs.permissionMode === "bypass" ? "full" : "ask");
  const pending = pendingModel?.sessionId === sessionId ? pendingModel : null;
  const knownLevels = useMemo(
    () => knownThinkingLevels(currentProvider, currentModel, modelThinking),
    [currentModel, currentProvider, modelThinking],
  );
  const thinkingLevels = knownLevels ?? thinkingLevelsFor(currentProvider, currentModel, modelThinking);

  /**
   * Every provider the menu can list, with whether it has a credential. The shipped catalog is the
   * fallback until `provider_list` answers, so the picker is never empty on first paint.
   */
  const catalog = useMemo(() => {
    const ids = registered.length > 0 ? registered.map((spec) => spec.id) : Object.keys(PROVIDERS);
    const configured = new Set(availableProviderIds(registered, providerStatuses));
    const groups = ids
      .filter((id) => prefs.enabledProviders[id] !== false)
      .map((id) => {
        const meta = providerMeta(id);
        const models = modelsByProvider[id]?.length ? modelsByProvider[id] : meta.models;
        return { id, meta, models, ready: configured.has(id) };
      });
    const owners = new Map<string, ModelChoice>();
    for (const group of groups) {
      for (const entry of group.models) {
        if (!owners.has(entry)) {
          owners.set(entry, { id: group.id, meta: group.meta, ready: group.ready });
        }
      }
    }
    return { groups, owners };
  }, [modelsByProvider, prefs.enabledProviders, providerStatuses, registered]);

  // A recent model that no longer exists, or whose provider lost its key, is not a choice any more.
  useEffect(() => {
    setRecentModels((current) => {
      const next = current
        .filter((id) => catalog.owners.get(id)?.ready)
        .filter((id, index, list) => list.indexOf(id) === index)
        .slice(0, RECENT_LIMIT);
      const same =
        next.length === current.length && next.every((id, index) => id === current[index]);
      return same ? current : next;
    });
  }, [catalog]);

  useEffect(() => {
    writeRecentModels(recentModels);
  }, [recentModels]);

  const modelMenu = useMemo(() => {
    const option = (value: string, owner: ModelChoice): ComboOption => ({
      value,
      label: modelLabel(value, owner.id),
      hint: owner.ready ? value : "Add a key in Settings",
      disabled: !owner.ready,
      icon: <ProviderGlyph provider={owner.id} />,
    });
    const groups: ComboGroup[] = [];
    const recent = recentModels.flatMap((id) => {
      const owner = catalog.owners.get(id);
      return owner ? [{ id, owner }] : [];
    });
    if (recent.length > 0) {
      groups.push({
        label: "Recent",
        options: recent.map((entry) => option(entry.id, entry.owner)),
      });
    }
    for (const entry of catalog.groups) {
      groups.push({
        label: entry.meta.name,
        options: entry.models.map((id) => option(id, entry)),
      });
    }
    return groups;
  }, [catalog, recentModels]);

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
    // `catalog.owners` is keyed by model id alone and first-wins, so a provider that also lists the
    // id would win over the session's own. Prefer the session's provider when it offers the id.
    const ownModels = modelsByProvider[detail.provider] ?? providerMeta(detail.provider).models;
    const owner = ownModels.includes(value)
      ? {
          id: detail.provider,
          meta: providerMeta(detail.provider),
          ready: available.includes(detail.provider),
        }
      : catalog.owners.get(value);
    if (!owner?.ready) return;
    let landed = true;
    if (owner.id === detail.provider) {
      // A same-provider change is a hot swap: keep the agent, its turn and its transcript. Compute
      // the nearest level the new model accepts from the capability map; with no entry, leave the
      // level exactly as the user had it.
      const levels = knownThinkingLevels(owner.id, value, modelThinking);
      const thinking = levels ? nearestThinkingLevel(currentThinking, levels) : null;
      if (!(await setModel(bridge, value, thinking))) return;
    } else {
      // A cross-provider change relaunches on the new wire client. While the session is busy or
      // waiting for an answer the store queues it and the shell applies it at idle, so there is no
      // toast yet: the composer shows the waiting dot and App toasts when it lands.
      landed = await switchModel(bridge, owner.id, value);
    }
    setRecentModels((current) =>
      [value, ...current.filter((id) => id !== value)].slice(0, RECENT_LIMIT),
    );
    if (landed) {
      notify(`Session now runs on ${owner.meta.name} · ${modelLabel(value, owner.id)}`);
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
            <div className="composer-pickers">
              <button
                type="button"
                className="inline-picker approval-picker"
                data-mode={permission}
                title={
                  permission === "ask"
                    ? "Ask before changes. Click for full access."
                    : "Full access: no approval prompts. Questions still ask."
                }
                aria-label={
                  permission === "ask"
                    ? "Approval mode: ask before changes. Switch to full access"
                    : "Approval mode: full access. Switch to ask"
                }
                onClick={() =>
                  void setPermissionMode(bridge, permission === "ask" ? "full" : "ask")
                }
              >
                <Icon name={permission === "ask" ? "shield" : "shieldCheck"} size="sm" />
              </button>
              <Combo
                className="inline-picker model-picker"
                id="model-select"
                label="Model for this session"
                value={currentModel}
                groups={modelMenu}
                disabled={busy}
                searchable
                placement="up"
                title={currentModel}
                triggerIcon={<ProviderGlyph provider={currentProvider} />}
                renderTriggerValue={(value) => modelLabel(value, currentProvider)}
                hideChevron
                footer={
                  <button
                    className="combo-footer-action"
                    type="button"
                    onClick={() => openSettings("providers")}
                  >
                    <Icon name="settings" size="xs" />
                    Providers
                  </button>
                }
                onChange={(value) => void changeModel(value)}
              />
              <Combo
                className="inline-picker thinking-picker"
                id="thinking-select"
                label="Thinking level for this session"
                value={currentThinking}
                groups={thinkingGroups(thinkingLevels)}
                disabled={busy}
                placement="up"
                title="Thinking level for the next turn"
                triggerIcon={<Icon name="brain" />}
                renderTriggerValue={(level) => thinkingLabel(level as ThinkingLevel)}
                hideChevron
                onChange={(value) => void setThinking(bridge, value as ThinkingLevel)}
              />
              {pending ? (
                <span
                  className="pending-switch"
                  role="status"
                  aria-label={`Switching to ${modelLabel(
                    pending.model,
                    pending.provider,
                  )} when this turn ends`}
                  title={`Waiting for this turn to finish before switching to ${modelLabel(
                    pending.model,
                    pending.provider,
                  )}`}
                >
                  <span className="dot" data-state="waiting" aria-hidden="true" />
                  {modelLabel(pending.model, pending.provider)}
                </span>
              ) : null}
            </div>
            <div className="foot-actions">
              <div className="send-stack">
                <button
                  className="btn-send"
                  type="button"
                  disabled={!typed || locked}
                  onClick={() => void submit()}
                  aria-label={working ? "Queue this message for the next turn" : "Send message"}
                  title={working && typed ? "Queue for the next turn" : "Send"}
                >
                  <Icon name={working && typed ? "arrowRight" : "send"} size="sm" />
                </button>
                {working ? (
                  <button
                    className="btn-stop"
                    type="button"
                    title="Stop"
                    aria-label="Stop the running turn"
                    onClick={() => void cancel(bridge)}
                  >
                    <Icon name="stop" size="sm" />
                  </button>
                ) : null}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
