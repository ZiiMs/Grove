import { describe, expect, it } from "vitest";
import { foldEvents } from "./transcript";

const AT = "2025-01-01T00:00:01.000Z";

describe("foldEvents", () => {
  it("folds deltas into one streaming bubble", () => {
    const items = foldEvents([
      ({ at: AT, kind: "delta", text: "Mock " }),
      ({ at: AT, kind: "delta", text: "reply" }),
    ]);

    expect(items).toEqual([{ id: "0:delta", kind: "stream", text: "Mock reply" }]);
  });

  it("replaces a streamed preview with the message that completes it, live or on replay", () => {
    const items = foldEvents([
      ({ at: AT, kind: "delta", text: "Mock reply: hi " }),
      ({
        at: AT,
        kind: "message",
        role: "assistant",
        blocks: [{ type: "text", text: "Mock reply: hi" }],
      }),
    ]);

    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "message", role: "assistant" });
    expect(JSON.stringify(items)).not.toContain("is-streaming");
  });

  it("keeps a partial reply when the stream was cancelled", () => {
    const items = foldEvents([
      ({ at: AT, kind: "delta", text: "Mock partial " }),
      ({ at: AT, kind: "status", state: "idle" }),
    ]);

    expect(items).toEqual([{ id: "0:delta", kind: "stream", text: "Mock partial " }]);
  });

  it("attaches a tool result to its call, making one card", () => {
    const items = foldEvents([
      ({
        at: AT,
        kind: "tool_call",
        call_id: "call-1",
        name: "echo",
        title: "echo hello",
      }),
      ({
        at: AT,
        kind: "tool_result",
        call_id: "call-1",
        status: "ok",
        summary: "echoed 5 chars",
      }),
    ]);

    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({
      kind: "tool",
      callId: "call-1",
      name: "echo",
      status: "ok",
      summary: "echoed 5 chars",
    });
  });

  it("ignores status events (they drive the header, not the transcript)", () => {
    expect(foldEvents([({ at: AT, kind: "status", state: "busy" })])).toEqual([]);
  });

  it("renders errors as an error notice", () => {
    const items = foldEvents([
      ({ at: AT, kind: "error", message: "harness exited", retryable: true }),
    ]);
    expect(items).toEqual([
      { id: "0:error", kind: "notice", tone: "error", text: "harness exited" },
    ]);
  });

  it("renders a context refresh as the visible system note it carries", () => {
    const items = foldEvents([
      {
        at: AT,
        kind: "context_refreshed",
        reason: "harness_switch",
        harness: "claude",
        documents: [{ id: "plan", title: "Plan", chars: 96, truncated: false }],
        note: "Harness switched from mock to claude; context re-sent: 1 document (96 chars)",
      },
    ]);
    expect(items).toEqual([
      {
        id: "0:context_refreshed",
        kind: "notice",
        tone: "info",
        text: "Harness switched from mock to claude; context re-sent: 1 document (96 chars)",
      },
    ]);
  });
});
