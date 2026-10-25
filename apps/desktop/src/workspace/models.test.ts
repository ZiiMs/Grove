import { describe, expect, it } from "vitest";
import { modelGroupsFor, providerForModel, providerMeta } from "./models";

describe("model to provider", () => {
  it("resolves a model the presentation map knows", () => {
    expect(providerForModel("claude-sonnet-4-5")).toBe("anthropic");
    expect(providerForModel("gpt-5")).toBe("openai");
  });

  it("prefers the groups the picker is showing, so a catalogue-only model is not misattributed", () => {
    // The vendor's catalogue can offer a model this build has never heard of. Reverse lookup in the
    // static map would then fall back to the wrong provider and start the session on the wrong vendor.
    const groups = [
      { provider: providerMeta("openrouter"), models: ["moonshotai/kimi-k2.6"] },
      { provider: providerMeta("openai"), models: ["gpt-5"] },
    ];
    expect(providerForModel("moonshotai/kimi-k2.6", groups)).toBe("openrouter");
  });

  it("still answers from the static map for a model no group lists", () => {
    const groups = [{ provider: providerMeta("openai"), models: ["gpt-5"] }];
    expect(providerForModel("claude-opus-4-1", groups)).toBe("anthropic");
  });

  it("is null for a model nobody offers, so the caller can fall back deliberately", () => {
    expect(providerForModel("not-a-model", [])).toBeNull();
  });

  it("knows the OpenCode Go fallback before the core answers", () => {
    const meta = providerMeta("opencode-go");
    expect(meta.name).toBe("OpenCode Go");
    expect(meta.models[0]).toBe("kimi-k3");
    expect(meta.models).toContain("glm-5.3");
  });

  it("groups the OpenCode Go models the core registers", () => {
    const groups = modelGroupsFor({}, ["opencode-go"], {
      "opencode-go": ["kimi-k3", "glm-5.3"],
    });
    expect(groups).toHaveLength(1);
    expect(groups[0].provider.name).toBe("OpenCode Go");
    expect(groups[0].models).toContain("glm-5.3");
  });
});
