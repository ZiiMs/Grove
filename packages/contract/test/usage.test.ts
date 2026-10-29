import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { planUsageSchema } from "../src/index";

function fixture(name: string): unknown {
  const path = fileURLToPath(new URL(`../../../contract/fixtures/${name}`, import.meta.url));
  return JSON.parse(readFileSync(path, "utf8"));
}

describe("plan usage", () => {
  it("parses the OpenCode Go fixture and keeps what is left", () => {
    const usage = planUsageSchema.parse(fixture("plan-usage.json"));
    expect(usage.provider).toBe("opencode-go");
    expect(usage.windows.map((window) => window.remainingPercent)).toEqual([94, 71, 26]);
    expect(usage.windows.map((window) => window.label)).toEqual(["5-hour", "Weekly", "Monthly"]);
  });
});
