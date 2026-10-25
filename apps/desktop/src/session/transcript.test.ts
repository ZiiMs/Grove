import { describe, expect, it } from "vitest";
import { foldTranscript, durationLabel } from "./transcript";

const AT = "2025-01-01T00:00:01.000Z";
const LATER = "2025-01-01T00:00:02.000Z";

describe("foldTranscript", () => {
  it("folds deltas into one streaming bubble", () => {
    const items = foldTranscript([
      { at: AT, kind: "delta", text: "Mock " },
      { at: LATER, kind: "delta", text: "reply" },
    ]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "stream", text: "Mock reply" });
  });

  it("replaces a streamed preview with the message that completes it", () => {
    const items = foldTranscript([
      { at: AT, kind: "delta", text: "Mock reply: hi " },
      {
        at: LATER,
        kind: "message",
        role: "assistant",
        blocks: [{ type: "text", text: "Mock reply: hi" }],
      },
    ]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "message", role: "assistant" });
  });

  it("replaces a streamed preview when usage arrives before the message", () => {
    const items = foldTranscript([
      { at: AT, kind: "delta", text: "OK" },
      { at: LATER, kind: "usage", input_tokens: 12, output_tokens: 2 },
      {
        at: LATER,
        kind: "message",
        role: "assistant",
        blocks: [{ type: "text", text: "OK" }],
      },
    ]);
    expect(items.filter((item) => item.kind === "message")).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "message", role: "assistant" });
    expect(items.some((item) => item.kind === "stream")).toBe(false);
    expect(items.filter((item) => item.kind === "usage")).toHaveLength(1);
  });

  it("keeps a partial reply when the stream was cancelled", () => {
    const items = foldTranscript([
      { at: AT, kind: "delta", text: "Mock partial " },
      { at: LATER, kind: "status", state: "idle" },
    ]);
    expect(items[0]).toMatchObject({ kind: "stream", text: "Mock partial " });
  });

  it("attaches a tool result to its call and records the elapsed time", () => {
    const items = foldTranscript([
      {
        at: AT,
        kind: "tool_call",
        call_id: "call-1",
        name: "echo",
        title: "echo hello",
      },
      {
        at: LATER,
        kind: "tool_result",
        call_id: "call-1",
        status: "ok",
        summary: "echoed 5 chars",
      },
    ]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({
      kind: "tool",
      callId: "call-1",
      name: "echo",
      status: "ok",
      summary: "echoed 5 chars",
      durationMs: 1000,
    });
  });

  it("matches a permission resolution to its request", () => {
    const items = foldTranscript([
      {
        at: AT,
        kind: "permission_request",
        request_id: "perm-1",
        title: "cargo test",
        detail: "Runs the suite",
      },
      {
        at: LATER,
        kind: "permission_resolved",
        request_id: "perm-1",
        decision: "approve",
      },
    ]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "permission", decision: "approve" });
  });

  it("folds a question request and its resolved answer into one row", () => {
    const items = foldTranscript([
      {
        at: AT,
        kind: "question_request",
        request_id: "q-1",
        questions: [
          {
            id: "scope",
            title: "Which scope?",
            hint: "",
            multi: false,
            other: true,
            options: [{ label: "Small", description: "Keep it tight." }],
          },
        ],
      },
      {
        at: LATER,
        kind: "question_resolved",
        request_id: "q-1",
        answers: [{ question_id: "scope", answers: [{ label: "Small", note: "Yes" }] }],
      },
    ]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "question", requestId: "q-1" });
    expect(items[0].kind === "question" ? items[0].answers : null).toHaveLength(1);
  });

  it("renders errors as an error notice and status as nothing", () => {
    const items = foldTranscript([
      { at: AT, kind: "status", state: "busy" },
      { at: LATER, kind: "error", message: "agent exited", retryable: true },
    ]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "notice", tone: "error", text: "agent exited" });
  });

  it("formats short and long durations", () => {
    expect(durationLabel(1400)).toBe("1.4s");
    expect(durationLabel(12_400)).toBe("12s");
    expect(durationLabel(72_000)).toBe("1m 12s");
    expect(durationLabel(null)).toBe("");
  });
});
