import { describe, expect, it } from "vitest";
import {
  durationLabel,
  foldTranscript,
  footerIndexes,
  toolTarget,
  toolTiming,
} from "./transcript";

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

  it("folds a completion across empty deltas and usage without duplicating the reply", () => {
    const items = foldTranscript([
      { at: AT, kind: "delta", text: "Okay" },
      { at: LATER, kind: "delta", text: " man" },
      { at: LATER, kind: "usage", input_tokens: 10, output_tokens: 2 },
      { at: LATER, kind: "delta", text: "" },
      {
        at: LATER,
        kind: "message",
        role: "assistant",
        blocks: [{ type: "text", text: "Okay man" }],
      },
      { at: LATER, kind: "status", state: "idle" },
    ]);
    expect(items.filter((item) => item.kind === "message")).toHaveLength(1);
    expect(items.some((item) => item.kind === "stream")).toBe(false);
    expect(items[0]).toMatchObject({ kind: "message", role: "assistant" });
  });

  it("keeps one preview when usage arrives mid-reply", () => {
    const items = foldTranscript([
      { at: AT, kind: "delta", text: "Okay man" },
      { at: LATER, kind: "usage", input_tokens: 1859, output_tokens: 4 },
      { at: LATER, kind: "delta", text: "2" },
      {
        at: LATER,
        kind: "message",
        role: "assistant",
        blocks: [{ type: "text", text: "Okay man2" }],
      },
    ]);
    expect(items.filter((item) => item.kind === "message")).toHaveLength(1);
    expect(items.some((item) => item.kind === "stream")).toBe(false);
    expect(items[0]).toMatchObject({ kind: "message", role: "assistant" });
  });

  it("skips whitespace streams when matching a completed reply", () => {
    const items = foldTranscript([
      { at: AT, kind: "delta", text: "Okay man" },
      { at: LATER, kind: "usage", input_tokens: 10, output_tokens: 2 },
      { at: LATER, kind: "delta", text: "  " },
      {
        at: LATER,
        kind: "message",
        role: "assistant",
        blocks: [{ type: "text", text: "Okay man" }],
      },
    ]);
    expect(items.filter((item) => item.kind === "message")).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "message", role: "assistant" });
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

  it("hides an empty refresh note but keeps a provider switch", () => {
    const items = foldTranscript([
      {
        at: AT,
        kind: "context_refreshed",
        reason: "refresh",
        provider: "opencode-go",
        documents: [],
        note: "Context refreshed for opencode-go: no documents",
      },
      {
        at: LATER,
        kind: "context_refreshed",
        reason: "provider_switch",
        provider: "anthropic",
        documents: [],
        note: "provider switched from opencode-go to anthropic; context re-sent: no documents",
      },
    ]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({
      kind: "notice",
      text: expect.stringContaining("provider switched"),
    });
  });

  it("renders errors as an error notice and status as nothing", () => {
    const items = foldTranscript([
      { at: AT, kind: "status", state: "busy" },
      { at: LATER, kind: "error", message: "agent exited", retryable: true },
    ]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "notice", tone: "error", text: "agent exited" });
  });

  it("puts one footer on the last assistant prose of a finished turn", () => {
    const items = foldTranscript([
      {
        at: AT,
        kind: "message",
        role: "user",
        blocks: [{ type: "text", text: "Fix the build." }],
      },
      {
        at: LATER,
        kind: "message",
        role: "assistant",
        blocks: [{ type: "text", text: "Checking the types." }],
      },
      { at: LATER, kind: "tool_call", call_id: "call-1", name: "Read", title: "src/lib.rs" },
      {
        at: LATER,
        kind: "tool_result",
        call_id: "call-1",
        status: "ok",
        summary: "read",
      },
      {
        at: LATER,
        kind: "message",
        role: "assistant",
        blocks: [{ type: "text", text: "Fixed." }],
      },
    ]);
    expect([...footerIndexes(items, false)]).toEqual([3]);
    expect([...footerIndexes(items, true)]).toEqual([]);
  });

  it("keeps the footer after a user message closes a turn that was still running", () => {
    const items = foldTranscript([
      {
        at: AT,
        kind: "message",
        role: "assistant",
        blocks: [{ type: "text", text: "Still working." }],
      },
      {
        at: LATER,
        kind: "message",
        role: "user",
        blocks: [{ type: "text", text: "Stop and try this instead." }],
      },
      {
        at: LATER,
        kind: "message",
        role: "assistant",
        blocks: [{ type: "text", text: "Done." }],
      },
    ]);
    expect([...footerIndexes(items, true)]).toEqual([0]);
    expect([...footerIndexes(items, false)]).toEqual([0, 2]);
  });

  it("gives a cancelled partial reply the footer and ignores whitespace streams", () => {
    const items = foldTranscript([
      { at: AT, kind: "delta", text: "   " },
      { at: LATER, kind: "tool_call", call_id: "call-1", name: "Read", title: "src/lib.rs" },
      { at: LATER, kind: "delta", text: "Partial answer" },
      { at: LATER, kind: "status", state: "idle" },
    ]);
    expect([...footerIndexes(items, false)]).toEqual([2]);
    expect(items[2]).toMatchObject({ kind: "stream", text: "Partial answer" });
  });

  it("formats short and long durations", () => {
    expect(durationLabel(1400)).toBe("1.4s");
    expect(durationLabel(12_400)).toBe("12s");
    expect(durationLabel(72_000)).toBe("1m 12s");
    expect(durationLabel(null)).toBe("");
  });
});

describe("tool rows", () => {
  it("strips the tool name the core prefixes onto a target", () => {
    expect(toolTarget("read_file", "read_file src/App.tsx")).toBe("src/App.tsx");
    expect(toolTarget("edit_file", "edit_file src/lib.rs")).toBe("src/lib.rs");
    expect(toolTarget("grep", "grep useState")).toBe("useState");
    expect(toolTarget("run_command", "Run cargo test")).toBe("Run cargo test");
    expect(toolTarget("subagent", "subagent · review the diff")).toBe("review the diff");
    expect(toolTarget("ask_user", "ask_user")).toBe("");
  });

  it("hides a timer on instant tools and keeps it on real work", () => {
    expect(toolTiming(null, null)).toBe("running");
    expect(toolTiming("ok", 0)).toBe("");
    expect(toolTiming("ok", 999)).toBe("");
    expect(toolTiming("ok", 1000)).toBe("1.0s");
    expect(toolTiming("ok", 12_400)).toBe("12s");
  });
});
