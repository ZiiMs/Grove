import type { Block, HarnessEvent, Role, ToolStatus } from "@grove/contract";

/**
 * What the conversation renders. Built by folding the persisted event list, so a reload rebuilds the
 * exact same conversation from SQLite — including the streamed reply, which is deduplicated against
 * the `message` that completes it.
 */
export type TranscriptItem =
  | { id: string; kind: "message"; role: Role; blocks: Block[] }
  | { id: string; kind: "stream"; text: string }
  | {
      id: string;
      kind: "tool";
      callId: string;
      name: string;
      title: string;
      status: ToolStatus | null;
      summary: string | null;
    }
  | { id: string; kind: "diff"; path: string; patch: string; added: number; removed: number }
  | { id: string; kind: "notice"; tone: "info" | "error"; text: string }
  | { id: string; kind: "usage"; inputTokens: number; outputTokens: number };

/** The text of a message, ignoring thinking and code blocks. */
export function messageText(blocks: Block[]): string {
  return blocks
    .filter((block): block is Extract<Block, { type: "text" }> => block.type === "text")
    .map((block) => block.text)
    .join("\n");
}

/**
 * Fold a transcript. Deltas accumulate into one streaming bubble; when the `message` that completes
 * them arrives (live, or replayed from SQLite after a reload) the preview is replaced by the message,
 * so the reply is never shown twice. Tool results attach to their call, making one card per call.
 */
export function foldEvents(events: HarnessEvent[]): TranscriptItem[] {
  const items: TranscriptItem[] = [];

  events.forEach((event, index) => {
    const id = `${index}:${event.kind}`;
    switch (event.kind) {
      case "delta": {
        const last = items[items.length - 1];
        if (last && last.kind === "stream") {
          last.text += event.text;
        } else {
          items.push({ id, kind: "stream", text: event.text });
        }
        break;
      }
      case "message": {
        const last = items[items.length - 1];
        const text = messageText(event.blocks);
        if (event.role === "assistant" && last && last.kind === "stream" && last.text.trimEnd() === text) {
          items[items.length - 1] = { id, kind: "message", role: event.role, blocks: event.blocks };
        } else {
          items.push({ id, kind: "message", role: event.role, blocks: event.blocks });
        }
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
        });
        break;
      case "tool_result": {
        for (let i = items.length - 1; i >= 0; i -= 1) {
          const item = items[i];
          if (item.kind === "tool" && item.callId === event.call_id) {
            items[i] = { ...item, status: event.status, summary: event.summary };
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
        });
        break;
      case "permission_request":
        items.push({
          id,
          kind: "notice",
          tone: "info",
          text: `Permission requested: ${event.title} — ${event.detail}`,
        });
        break;
      case "permission_resolved":
        items.push({
          id,
          kind: "notice",
          tone: "info",
          text: `Permission ${event.decision}: ${event.request_id}`,
        });
        break;
      case "usage":
        items.push({
          id,
          kind: "usage",
          inputTokens: event.input_tokens,
          outputTokens: event.output_tokens,
        });
        break;
      case "error":
        items.push({ id, kind: "notice", tone: "error", text: event.message });
        break;
      case "session_started":
        items.push({
          id,
          kind: "notice",
          tone: "info",
          text: `${event.harness} started${event.resumed ? " (resumed)" : ""}`,
        });
        break;
      case "status":
        // Status drives the session header and list, not the transcript.
        break;
    }
  });

  return items;
}
