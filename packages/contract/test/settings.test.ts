import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  providerStatusSchema,
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
    expect(parsed.catalogSources).toEqual(["anthropics/skills", "mattpocock/skills"]);
    expect(parsed.titleModel).toEqual({
      provider: "anthropic",
      model: "claude-haiku-4-5",
    });
    expect(parsed.futureFlag).toBe(true);
  });

  it("reads a cleared title model and keeps clearing explicit", () => {
    const nulled = settingsSchema.parse({
      catalogSources: [],
      theme: "system",
      titleModel: null,
    });
    expect(nulled.titleModel).toBeNull();

    const patch = settingsPatchSchema.parse({ clearTitleModel: true });
    expect(patch.clearTitleModel).toBe(true);
    expect(() => settingsPatchSchema.parse({ titleModel: null })).toThrow();
  });

  it("rejects a settings document with a missing typed field", () => {
    expect(() => settingsSchema.parse({ theme: "dark" })).toThrow();
  });

  it("rejects an unknown theme and unknown patch fields are accepted", () => {
    expect(() => settingsSchema.parse({ catalogSources: [], theme: "neon" })).toThrow();
    const patch = settingsPatchSchema.parse({ theme: "light", futureFlag: 1 });
    expect(patch.theme).toBe("light");
    expect(patch.futureFlag).toBe(1);
  });

  it("secret-status.json carries a key and a status, never a value", () => {
    const parsed = secretStatusViewSchema.parse(fixture("secret-status.json"));
    expect(parsed).toEqual({ key: "github.token", status: "present" });
    expect(Object.keys(parsed)).toHaveLength(2);
  });

  it("provider-statuses.json is the settings pane's row, without a key", () => {
    const rows = (fixture("provider-statuses.json") as unknown[]).map((row) =>
      providerStatusSchema.parse(row),
    );
    expect(rows.map((row) => row.id)).toEqual(["anthropic", "openai"]);
    expect(rows[0].configured).toBe(true);
    expect(rows[0].source).toBe("keychain");
    expect(rows[1].configured).toBe(false);
    expect(rows[1].detail).toContain("Add an API key");
    expect(Object.keys(rows[0])).not.toContain("key");
  });

  it("rejects a provider row without a configured flag", () => {
    expect(() =>
      providerStatusSchema.parse({
        id: "anthropic",
        name: "Anthropic",
        short: "AN",
        tint: "#c08a63",
        method: "api_key",
        keyUrl: "https://console.anthropic.com/settings/keys",
        envVar: "ANTHROPIC_API_KEY",
        defaultModel: "claude-sonnet-4-5",
        models: ["claude-sonnet-4-5"],
        source: null,
        detail: null,
      }),
    ).toThrow();
  });
});
