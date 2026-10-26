import type {
  Block,
  Decision,
  Question,
  QuestionAnswer,
  SessionEvent,
  Role,
  ToolStatus,
} from "@grove/contract";

/** One row of the transcript, folded from the persisted event log. */
export type TranscriptItem =
  | {
      id: string;
      kind: "message";
      role: Role;
      blocks: Block[];
      at: string;
      durationMs: number | null;
    }
  | { id: string; kind: "stream"; text: string; at: string }
  | {
      id: string;
      kind: "tool";
      callId: string;
      name: string;
      title: string;
      status: ToolStatus | null;
      summary: string | null;
      at: string;
      durationMs: number | null;
    }
  | {
      id: string;
      kind: "diff";
      path: string;
      patch: string;
      added: number;
      removed: number;
      at: string;
    }
  | {
      id: string;
      kind: "permission";
      requestId: string;
      title: string;
      detail: string;
      decision: Decision | null;
      at: string;
    }
  | { id: string; kind: "notice"; tone: "info" | "error"; text: string; at: string }
  | {
      id: string;
      kind: "question";
      requestId: string;
      questions: Question[];
      answers: QuestionAnswer[] | null;
      at: string;
    }
  | {
      id: string;
      kind: "usage";
      inputTokens: number;
      outputTokens: number;
      at: string;
    };

export function messageText(blocks: Block[]): string {
  return blocks
    .filter((block): block is Extract<Block, { type: "text" }> => block.type === "text")
    .map((block) => block.text)
    .join("\n");
}

function elapsed(from: string, to: string): number | null {
  const start = Date.parse(from);
  const end = Date.parse(to);
  if (Number.isNaN(start) || Number.isNaN(end) || end < start) return null;
  return end - start;
}

/**
 * Fold the event log into rows. Deltas accumulate into one streaming bubble and are replaced by the
 * message that completes them, so a live turn and a replayed one look identical. Tool results attach
 * to their call, which is what makes one card per tool.
 */
export function foldTranscript(events: SessionEvent[]): TranscriptItem[] {
  const items: TranscriptItem[] = [];

  events.forEach((event, index) => {
    const id = `${index}:${event.kind}`;
    const previousAt = items.length > 0 ? items[items.length - 1].at : null;
    const sincePrevious = previousAt ? elapsed(previousAt, event.at) : null;
    switch (event.kind) {
      case "delta": {
        const last = items[items.length - 1];
        if (last && last.kind === "stream") {
          last.text += event.text;
        } else {
          items.push({ id, kind: "stream", text: event.text, at: event.at });
        }
        break;
      }
      case "message": {
        const text = messageText(event.blocks);
        // A usage event lands between the last delta and the final assistant message, so the stream
        // is not necessarily the last row. Skip those invisible rows when looking for the preview.
        let streamIndex = -1;
        if (event.role === "assistant") {
          for (let i = items.length - 1; i >= 0; i -= 1) {
            const item = items[i];
            if (item.kind === "usage") continue;
            if (item.kind === "stream" && item.text.trimEnd() === text) streamIndex = i;
            break;
          }
        }
        if (streamIndex >= 0) {
          const stream = items[streamIndex];
          if (stream.kind === "stream") {
            const previousAt = streamIndex > 0 ? items[streamIndex - 1].at : null;
            items[streamIndex] = {
              id,
              kind: "message",
              role: event.role,
              blocks: event.blocks,
              at: stream.at,
              durationMs: previousAt ? elapsed(previousAt, event.at) : null,
            };
            break;
          }
        }
        items.push({
          id,
          kind: "message",
          role: event.role,
          blocks: event.blocks,
          at: event.at,
          durationMs: sincePrevious,
        });
        break;
      }
      case "tool_call":
        items.push({
          id,
          kind: "tool",
          callId: event.call_id,
          name: event.name,
          title: event.title,
          status: null,
          summary: null,
          at: event.at,
          durationMs: null,
        });
        break;
      case "tool_result": {
        for (let i = items.length - 1; i >= 0; i -= 1) {
          const item = items[i];
          if (item.kind === "tool" && item.callId === event.call_id) {
            items[i] = {
              ...item,
              status: event.status,
              summary: event.summary,
              durationMs: elapsed(item.at, event.at),
            };
            break;
          }
        }
        break;
      }
      case "diff":
        items.push({
          id,
          kind: "diff",
          path: event.path,
          patch: event.patch,
          added: event.added,
          removed: event.removed,
          at: event.at,
        });
        break;
      case "permission_request":
        items.push({
          id,
          kind: "permission",
          requestId: event.request_id,
          title: event.title,
          detail: event.detail,
          decision: null,
          at: event.at,
        });
        break;
      case "permission_resolved": {
        let matched = false;
        for (let i = items.length - 1; i >= 0; i -= 1) {
          const item = items[i];
          if (item.kind === "permission" && item.requestId === event.request_id) {
            items[i] = { ...item, decision: event.decision };
            matched = true;
            break;
          }
        }
        if (!matched) {
          items.push({
            id,
            kind: "notice",
            tone: "info",
            text: `Permission ${event.decision}: ${event.request_id}`,
            at: event.at,
          });
        }
        break;
      }
      case "question_request":
        items.push({
          id,
          kind: "question",
          requestId: event.request_id,
          questions: event.questions,
          answers: null,
          at: event.at,
        });
        break;
      case "question_resolved": {
        for (let i = items.length - 1; i >= 0; i -= 1) {
          const item = items[i];
          if (item.kind === "question" && item.requestId === event.request_id) {
            items[i] = { ...item, answers: event.answers };
            break;
          }
        }
        break;
      }
      case "usage":
        items.push({
          id,
          kind: "usage",
          inputTokens: event.input_tokens,
          outputTokens: event.output_tokens,
          at: event.at,
        });
        break;
      case "error":
        items.push({ id, kind: "notice", tone: "error", text: event.message, at: event.at });
        break;
      case "session_started":
        items.push({
          id,
          kind: "notice",
          tone: "info",
          text: `${event.provider} started${event.resumed ? " (resumed)" : ""}`,
          at: event.at,
        });
        break;
      case "status":
        break;
      case "context_refreshed":
        // A model change used to write an empty refresh note on every picker change. Keep
        // provider-switch notices, but an empty refresh has nothing useful to say.
        if (event.reason === "refresh" && event.documents.length === 0) break;
        items.push({ id, kind: "notice", tone: "info", text: event.note, at: event.at });
        break;
    }
  });

  return items;
}

/** A short elapsed label for a tool card or a turn. */
export function durationLabel(ms: number | null): string {
  if (ms === null) return "";
  if (ms < 10_000) return `${(ms / 1000).toFixed(1)}s`;
  if (ms < 60_000) return `${Math.round(ms / 1000)}s`;
  const minutes = Math.floor(ms / 60_000);
  const seconds = Math.round((ms % 60_000) / 1000);
  return `${minutes}m ${seconds}s`;
}

export function clockLabel(at: string): string {
  const date = new Date(at);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleTimeString(undefined, { hour12: false });
}
