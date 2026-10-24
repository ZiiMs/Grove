import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  doctorReportSchema,
  secretStatusViewSchema,
  settingsPatchSchema,
  settingsSchema,
} from "../src/index";

function fixture(name: string): unknown {
  const path = fileURLToPath(new URL(`../../../contract/fixtures/${name}`, import.meta.url));
  return JSON.parse(readFileSync(path, "utf8"));
}

describe("settings contract fixtures", () => {
  it("settings.json matches the zod schema and keeps the unknown key", () => {
    const parsed = settingsSchema.parse(fixture("settings.json"));
    expect(parsed.theme).toBe("dark");
    expect(parsed.harnessPaths.claude).toBe("C:/Tools/claude.exe");
    expect(parsed.catalogSources).toEqual(["anthropics/skills", "mattpocock/skills"]);
    expect(parsed.futureFlag).toBe(true);
  });

  it("rejects a settings document with a missing typed field", () => {
    expect(() =>
      settingsSchema.parse({ harnessPaths: {}, theme: "dark" }),
    ).toThrow();
  });

  it("rejects an unknown theme and unknown patch fields are accepted", () => {
    expect(() =>
      settingsSchema.parse({ harnessPaths: {}, catalogSources: [], theme: "neon" }),
    ).toThrow();
    const patch = settingsPatchSchema.parse({ theme: "light", futureFlag: 1 });
    expect(patch.theme).toBe("light");
    expect(patch.futureFlag).toBe(1);
  });

  it("secret-status.json carries a key and a status, never a value", () => {
    const parsed = secretStatusViewSchema.parse(fixture("secret-status.json"));
    expect(parsed).toEqual({ key: "github.token", status: "present" });
    expect(Object.keys(parsed)).toHaveLength(2);
  });

  it("doctor.json matches the zod schema", () => {
    const parsed = doctorReportSchema.parse(fixture("doctor.json"));
    expect(parsed.harnesses.map((harness) => harness.id)).toEqual(["mock", "claude"]);
    expect(parsed.harnesses[0].installed).toBe(true);
    expect(parsed.harnesses[1].installed).toBe(false);
    expect(parsed.harnesses[1].detail).toContain("configured path does not exist");
    expect(parsed.tools.map((tool) => tool.id)).toEqual(["node", "git", "gh"]);
    expect(parsed.tools[2].installed).toBe(false);
    expect(parsed.checkedAt).toBe("2026-10-24T18:00:00.000Z");
  });

  it("rejects a doctor row without an installed flag", () => {
    expect(() =>
      doctorReportSchema.parse({
        harnesses: [{ id: "mock", label: "Mock", version: null, path: null, detail: null }],
        tools: [],
        checkedAt: "2026-10-24T18:00:00.000Z",
      }),
    ).toThrow();
  });
});
