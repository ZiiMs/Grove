import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  contextDocumentSchema,
  contextLimitsSchema,
  contextPinSchema,
  sessionContextSchema,
} from "../src/index";

function fixture(name: string): unknown {
  const path = fileURLToPath(new URL(`../../../contract/fixtures/${name}`, import.meta.url));
  return JSON.parse(readFileSync(path, "utf8"));
}

describe("session context fixtures", () => {
  it("session-context.json matches the zod schema", () => {
    const parsed = sessionContextSchema.parse(fixture("session-context.json"));
    expect(parsed.sessionId).toBe("session-1");
    expect(parsed.harness).toBe("claude");
    expect(parsed.planVersion).toBe(3);
    expect(parsed.totalChars).toBe(1776);
    expect(parsed.limits).toEqual({ perDocChars: 8000, totalChars: 24000 });
    expect(parsed.pins.map((pin) => pin.specId)).toEqual([
      "grove-architecture",
      "grove-goal-and-requirements",
    ]);
    expect(parsed.documents.map((doc) => doc.id)).toEqual([
      "grove-architecture",
      "grove-goal-and-requirements",
      "plan",
    ]);
    expect(parsed.documents[1].truncated).toBe(true);
  });
});

describe("context budgets and pins", () => {
  it("a pin is a spec id plus the title the header shows", () => {
    expect(
      contextPinSchema.parse({ specId: "grove-architecture", title: "Grove — architecture" }),
    ).toEqual({ specId: "grove-architecture", title: "Grove — architecture" });
  });

  it("a document records what the harness received, marker included", () => {
    expect(
      contextDocumentSchema.parse({
        id: "plan",
        title: "Plan",
        chars: 96,
        truncated: false,
      }),
    ).toEqual({ id: "plan", title: "Plan", chars: 96, truncated: false });
  });

  it("limits are positive character budgets", () => {
    expect(contextLimitsSchema.safeParse({ perDocChars: 0, totalChars: 24000 }).success).toBe(false);
    expect(contextLimitsSchema.safeParse({ perDocChars: 8000 }).success).toBe(false);
    expect(
      contextLimitsSchema.parse({ perDocChars: 8000, totalChars: 24000 }),
    ).toEqual({ perDocChars: 8000, totalChars: 24000 });
  });

  it("rejects a snapshot without a session id", () => {
    const raw = fixture("session-context.json") as Record<string, unknown>;
    const { sessionId: _sessionId, ...rest } = raw;
    expect(sessionContextSchema.safeParse(rest).success).toBe(false);
  });
});
