import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { parsePlanChanged, planSchema, planUpdateSchema } from "../src/index";

function fixture(name: string): unknown {
  const path = fileURLToPath(new URL(`../../../contract/fixtures/${name}`, import.meta.url));
  return JSON.parse(readFileSync(path, "utf8"));
}

describe("plan contract fixtures", () => {
  it("plan.json matches the zod schema", () => {
    const parsed = planSchema.parse(fixture("plan.json"));
    expect(parsed.sessionId).toBe("session-1");
    expect(parsed.version).toBe(3);
    expect(parsed.markdown).toBe("# Plan\n\n- ship ticket 09\n");
    expect(parsed.mirrorPath).toBe("C:/projects/grove/.grove/sessions/session-1/plan.md");
  });

  it("plan-update.json pins the save arguments", () => {
    const parsed = planUpdateSchema.parse(fixture("plan-update.json"));
    expect(parsed).toEqual({
      markdown: "# Plan\n\n- ship ticket 09\n",
      expectedVersion: 3,
    });
  });

  it("plan-changed.json is an envelope carrying an imported plan", () => {
    const parsed = parsePlanChanged(fixture("plan-changed.json"));
    expect(parsed).not.toBeNull();
    expect(parsed?.sessionId).toBe("session-1");
    expect(parsed?.payload.version).toBe(3);
    expect(parsed?.payload.sessionId).toBe(parsed?.sessionId);
  });
});

describe("plan schemas", () => {
  it("reject a negative version instead of guessing", () => {
    expect(
      planSchema.safeParse({
        sessionId: "session-1",
        markdown: "",
        version: -1,
        mirrorPath: "plan.md",
      }).success,
    ).toBe(false);
  });

  it("reject a save with no version guard", () => {
    expect(planUpdateSchema.safeParse({ markdown: "hi" }).success).toBe(false);
  });

  it("drop a malformed plan-changed payload", () => {
    expect(parsePlanChanged({ sessionId: "session-1", payload: { markdown: "hi" } })).toBeNull();
    expect(parsePlanChanged(null)).toBeNull();
  });
});
