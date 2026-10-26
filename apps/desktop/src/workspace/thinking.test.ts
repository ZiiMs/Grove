import { describe, expect, it } from "vitest";
import {
  STATIC_THINKING_LEVELS,
  knownThinkingLevels,
  nearestThinkingLevel,
  thinkingGroups,
  thinkingLabel,
  thinkingLevelsFor,
  type ModelThinkingMap,
} from "./thinking";

const capabilities: ModelThinkingMap = {
  "opencode-go": {
    "kimi-k3.1-pro": ["low", "high", "max"],
    "kimi-k3": ["max"],
  },
};

describe("model thinking capabilities", () => {
  it("uses the model's catalogue levels and falls back only when it has no entry", () => {
    expect(knownThinkingLevels("opencode-go", "kimi-k3.1-pro", capabilities)).toEqual([
      "low",
      "high",
      "max",
    ]);
    expect(knownThinkingLevels("opencode-go", "unknown", capabilities)).toBeNull();
    expect(thinkingLevelsFor("opencode-go", "unknown", capabilities)).toEqual([
      ...STATIC_THINKING_LEVELS,
    ]);
    expect(knownThinkingLevels("other", "kimi-k3.1-pro", capabilities)).toBeNull();
  });

  it("moves a saved level to the nearest supported one", () => {
    expect(nearestThinkingLevel("medium", ["off", "low", "high"])).toBe("high");
    expect(nearestThinkingLevel("max", ["off", "low", "high"])).toBe("high");
    expect(nearestThinkingLevel("off", ["max"])).toBe("max");
    expect(nearestThinkingLevel("high", ["low", "high", "max"])).toBe("high");
    expect(nearestThinkingLevel("off", ["none", "low"])).toBe("none");
    expect(nearestThinkingLevel("none", ["off", "low"])).toBe("off");
  });

  it("labels every tier and builds display groups in order", () => {
    expect(thinkingLabel("xhigh")).toBe("Extra high");
    expect(thinkingGroups(["off", "max"])[0].options).toEqual([
      { value: "off", label: "Off" },
      { value: "max", label: "Max" },
    ]);
  });
});
