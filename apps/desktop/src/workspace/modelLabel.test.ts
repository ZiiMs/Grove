import { describe, expect, it } from "vitest";
import { PROVIDERS } from "./models";
import { modelLabel } from "./modelLabel";

/**
 * The expected label for every id in the shipped catalog. Kept as literal product names, never
 * derived from the formatter, so a change in either the catalogue or the rules is visible here.
 */
const CATALOG: Record<string, string> = {
  "claude-sonnet-4-5": "Sonnet 4.5",
  "claude-opus-4-1": "Opus 4.1",
  "claude-haiku-4-5": "Haiku 4.5",
  "gpt-5": "GPT-5",
  "gpt-5-mini": "GPT-5 Mini",
  "o4-mini": "O4 Mini",
  "gemini-2.5-pro": "Gemini 2.5 Pro",
  "gemini-2.5-flash": "Gemini 2.5 Flash",
  "anthropic/claude-sonnet-4.5": "Sonnet 4.5",
  "openai/gpt-5": "GPT-5",
  "google/gemini-2.5-pro": "Gemini 2.5 Pro",
  "kimi-k3": "Kimi K3",
  "kimi-k3.1-pro": "Kimi K3.1 Pro",
  "glm-5.3": "GLM-5.3",
  "glm-5.3-flash": "GLM-5.3 Flash",
  "glm-5.2": "GLM-5.2",
  "deepseek-v4.1-flash": "DeepSeek V4.1 Flash",
  "deepseek-v4-pro": "DeepSeek V4 Pro",
  "minimax-m3.1-flash": "MiniMax M3.1 Flash",
  "mimo-v2.6-pro": "MiMo V2.6 Pro",
  "mimo-v2.6-flash": "MiMo V2.6 Flash",
  "mimo-v2.5-pro": "MiMo V2.5 Pro",
  "mimo-v2.5": "MiMo V2.5",
  "kimi-k2.7-code": "Kimi K2.7 Code",
  "longcat-2.0": "Longcat 2.0",
  "step-5-preview": "Step 5 Preview",
  "hy4-preview": "Hy4 Preview",
  "hy3": "Hy3",
  "beam-free": "Beam Free",
  "mock-1": "Mock 1",
};

function catalogIds(): string[] {
  return Object.values(PROVIDERS).flatMap((provider) => provider.models);
}

describe("modelLabel", () => {
  it("names every static catalog id", () => {
    expect(catalogIds().sort()).toEqual(Object.keys(CATALOG).sort());
  });

  it.each(Object.entries(CATALOG))("labels %s as %s", (id, expected) => {
    const provider = Object.values(PROVIDERS).find((entry) => entry.models.includes(id))?.id ?? null;
    expect(modelLabel(id, provider)).toBe(expected);
  });

  it.each([
    ["claude-sonnet-4-5-20250929", "Sonnet 4.5"],
    ["claude-haiku-4-5-20251001", "Haiku 4.5"],
    ["gpt-5-mini-2025-08-07", "GPT-5 Mini"],
    ["o4-mini-2025-04-16", "O4 Mini"],
    ["gemini-2.5-flash-20250520", "Gemini 2.5 Flash"],
    ["deepseek-v4.1-flash-20260101", "DeepSeek V4.1 Flash"],
    ["kimi-k3-20260115", "Kimi K3"],
    ["anthropic/claude-sonnet-4-5-latest", "Sonnet 4.5"],
    ["openai/gpt-5-mini-2025-08-07", "GPT-5 Mini"],
  ])("strips the date stamp from the live-style id %s", (id, expected) => {
    expect(modelLabel(id, "openrouter")).toBe(expected);
  });

  it("keeps the provider's own family name out of the label", () => {
    expect(modelLabel("claude-sonnet-4-5", "anthropic")).toBe("Sonnet 4.5");
    expect(modelLabel("claude-sonnet-4-5", "openrouter")).toBe("Sonnet 4.5");
    expect(modelLabel("anthropic/claude-sonnet-4.5", "openrouter")).toBe("Sonnet 4.5");
  });

  it("leaves a model it does not recognise readable instead of raw", () => {
    expect(modelLabel("llama-4-70b", "openrouter")).toBe("Llama 4 70b");
    expect(modelLabel("some-new-model-20260101", "openrouter")).toBe("Some New Model");
  });

  it("falls back to the exact id when nothing is left", () => {
    expect(modelLabel("claude-", "anthropic")).toBe("claude-");
    expect(modelLabel("", "anthropic")).toBe("");
  });
});
