import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  harnessEventSchema,
  sessionDetailSchema,
  sessionSchema,
  sessionSummarySchema,
} from "../src/index";

function fixture(name: string): unknown {
  const path = fileURLToPath(new URL(`../../../contract/fixtures/${name}`, import.meta.url));
  return JSON.parse(readFileSync(path, "utf8"));
}

describe("session contract fixtures", () => {
  it("session.json matches the zod schema", () => {
    const parsed = sessionSchema.parse(fixture("session.json"));
    expect(parsed.id).toBe("session-1");
    expect(parsed.harness).toBe("mock");
    expect(parsed.status).toBe("idle");
    expect(parsed.createdAt).toBe("2025-01-01T00:00:00.000Z");
  });

  it("session-summary.json flattens the session and carries the event count", () => {
    const parsed = sessionSummarySchema.parse(fixture("session-summary.json"));
    expect(parsed.id).toBe("session-1");
    expect(parsed.eventCount).toBe(12);
  });

  it("session-detail.json carries a transcript of normalized events", () => {
    const parsed = sessionDetailSchema.parse(fixture("session-detail.json"));
    expect(parsed.events.map((event) => event.kind)).toEqual(["session_started", "message"]);
    expect(parsed.events[1]).toMatchObject({ kind: "message", role: "assistant" });
  });

  it("session-events.json covers every event payload variant", () => {
    const raw = fixture("session-events.json") as unknown[];
    const events = raw.map((event) => harnessEventSchema.parse(event));
    expect(events.map((event) => event.kind)).toEqual([
      "session_started",
      "status",
      "delta",
      "message",
      "tool_call",
      "tool_result",
      "diff",
      "permission_request",
      "permission_resolved",
      "usage",
      "status",
      "error",
    ]);
    const toolCall = events[4];
    expect(toolCall).toMatchObject({ kind: "tool_call", call_id: "mock-call-1", name: "echo" });
    const toolResult = events[5];
    expect(toolResult).toMatchObject({ kind: "tool_result", status: "ok" });
  });
});

describe("harness events", () => {
  it("rejects an unknown kind instead of guessing", () => {
    expect(
      harnessEventSchema.safeParse({ at: "2025-01-01T00:00:00.000Z", kind: "from-the-future" })
        .success,
    ).toBe(false);
  });

  it("requires a timestamp", () => {
    expect(harnessEventSchema.safeParse({ kind: "delta", text: "hi" }).success).toBe(false);
  });

  it("rejects an unknown session state", () => {
    expect(
      sessionSchema.safeParse({
        ...(fixture("session.json") as Record<string, unknown>),
        status: "paused",
      }).success,
    ).toBe(false);
  });
});
