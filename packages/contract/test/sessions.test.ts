import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  capabilitiesSchema,
  createSessionRequestSchema,
  detectionSchema,
  providerSpecSchema,
  providerStatusSchema,
  sessionDetailSchema,
  sessionEventSchema,
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
    expect(parsed.provider).toBe("mock");
    expect(parsed.status).toBe("idle");
    expect(parsed.createdAt).toBe("2025-01-01T00:00:00.000Z");
    expect(parsed.archivedAt).toBeNull();
    expect(parsed.titleSource).toBe("manual");
  });

  it("an archived session keeps a timestamp the tree can filter on", () => {
    const base = fixture("session.json") as Record<string, unknown>;
    const parsed = sessionSchema.parse({
      ...base,
      archivedAt: "2025-01-02T00:00:00.000Z",
    });
    expect(parsed.archivedAt).toBe("2025-01-02T00:00:00.000Z");
  });

  it("session-summary.json flattens the session, its count and its workers", () => {
    const parsed = sessionSummarySchema.parse(fixture("session-summary.json"));
    expect(parsed.id).toBe("session-1");
    expect(parsed.eventCount).toBe(12);
    expect(parsed.subagents).toEqual([
      {
        callId: "call_sub_1",
        description: "audit the store",
        status: "ok",
        summary: 'subagent "audit the store" finished.\n\nThe store is consistent.',
        startedAt: "2025-01-01T00:00:00.000Z",
        finishedAt: "2025-01-01T00:00:05.000Z",
      },
      {
        callId: "call_sub_2",
        description: "check the tests",
        status: "running",
        summary: null,
        startedAt: "2025-01-01T00:00:06.000Z",
        finishedAt: null,
      },
    ]);
  });

  it("a legacy session summary without subagents defaults to none", () => {
    const base = fixture("session-summary.json") as Record<string, unknown>;
    const { subagents: _omitted, ...legacy } = base;
    expect(sessionSummarySchema.parse(legacy).subagents).toEqual([]);
  });

  it("session-detail.json carries a transcript of normalized events", () => {
    const parsed = sessionDetailSchema.parse(fixture("session-detail.json"));
    expect(parsed.events.map((event) => event.kind)).toEqual(["session_started", "message"]);
    expect(parsed.events[1]).toMatchObject({ kind: "message", role: "assistant" });
  });

  it("session-events.json covers every event payload variant", () => {
    const raw = fixture("session-events.json") as unknown[];
    const events = raw.map((event) => sessionEventSchema.parse(event));
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
      "question_request",
      "question_resolved",
      "usage",
      "status",
      "title_changed",
      "error",
      "context_refreshed",
    ]);
    const toolCall = events[4];
    expect(toolCall).toMatchObject({ kind: "tool_call", call_id: "mock-call-1", name: "echo" });
    const toolResult = events[5];
    expect(toolResult).toMatchObject({ kind: "tool_result", status: "ok" });
    expect(events[13]).toMatchObject({
      kind: "title_changed",
      title: "Release notes",
      source: "manual",
    });
  });

  it("session-events.json records the context a session was given", () => {
    const raw = fixture("session-events.json") as unknown[];
    const refresh = sessionEventSchema.parse(raw[raw.length - 1]);
    expect(refresh).toMatchObject({
      kind: "context_refreshed",
      reason: "refresh",
      provider: "mock",
    });
    expect(
      refresh.kind === "context_refreshed" ? refresh.documents : [],
    ).toEqual([
      {
        id: "grove-architecture",
        title: "Grove — architecture",
        chars: 1200,
        truncated: false,
      },
    ]);
  });
});

describe("provider contract", () => {
  it("capabilities.json matches the zod schema", () => {
    const parsed = capabilitiesSchema.parse(fixture("capabilities.json"));
    expect(parsed).toEqual({
      streamingDeltas: true,
      toolCalls: true,
      diffs: false,
      permissions: false,
      resume: true,
      images: false,
      mcp: false,
      usage: true,
    });
  });

  it("provider-info.json describes a vendor the picker can offer", () => {
    const parsed = providerSpecSchema.parse(fixture("provider-info.json"));
    expect(parsed.id).toBe("anthropic");
    expect(parsed.wire).toBe("anthropic");
    expect(parsed.auth).toBe("api_key");
    expect(parsed.models).toContain(parsed.defaultModel);
    expect(parsed.capabilities.toolCalls).toBe(true);
  });

  it("provider-statuses.json reports readiness without a key value", () => {
    const raw = fixture("provider-statuses.json") as unknown[];
    const rows = raw.map((row) => providerStatusSchema.parse(row));
    expect(rows[0]).toMatchObject({ id: "anthropic", configured: true, source: "keychain" });
    expect(rows[1]).toMatchObject({ id: "openai", configured: false, source: null });
    expect(Object.keys(raw[0] as object)).not.toContain("key");
  });

  it("create-session-request.json names a provider and its config", () => {
    const parsed = createSessionRequestSchema.parse(fixture("create-session-request.json"));
    expect(parsed.provider).toBe("anthropic");
    expect(parsed.config).toEqual({ maxSteps: 12 });
  });

  it("a create request without a config is still valid", () => {
    const parsed = createSessionRequestSchema.parse({ provider: "anthropic" });
    expect(parsed.config).toBeUndefined();
  });

  it("detection.json reports whether a provider would start", () => {
    const parsed = detectionSchema.parse(fixture("detection.json"));
    expect(parsed).toEqual({
      installed: true,
      method: "api_key",
      defaultModel: "claude-sonnet-4-5",
      detail: null,
    });
  });

  it("a provider without a key is a valid detection with a reason", () => {
    const parsed = detectionSchema.parse({
      installed: false,
      method: "api_key",
      defaultModel: "gpt-5",
      detail: "No API key for OpenAI.",
    });
    expect(parsed.installed).toBe(false);
    expect(parsed.detail).toBe("No API key for OpenAI.");
  });
});

describe("session events", () => {
  it("rejects an unknown kind instead of guessing", () => {
    expect(
      sessionEventSchema.safeParse({ at: "2025-01-01T00:00:00.000Z", kind: "from-the-future" })
        .success,
    ).toBe(false);
  });

  it("requires a timestamp", () => {
    expect(sessionEventSchema.safeParse({ kind: "delta", text: "hi" }).success).toBe(false);
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
