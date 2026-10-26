import type { ThinkingLevel } from "@grove/contract";

/** The fallback used when the capability catalogue has no entry for a model. */
export const STATIC_THINKING_LEVELS: readonly ThinkingLevel[] = ["off", "low", "medium", "high"];

/** Provider id → model id → the levels that model accepts. Empty means "use the fallback". */
export type ModelThinkingMap = Record<string, Record<string, ThinkingLevel[]>>;

const LABELS: Record<ThinkingLevel, string> = {
  off: "Off",
  none: "Off",
  minimal: "Minimal",
  low: "Low",
  medium: "Medium",
  high: "High",
  xhigh: "Extra high",
  max: "Max",
};

/** Display order and proximity rank; matches the Rust `ThinkingLevel` declaration order. */
const RANK: Record<ThinkingLevel, number> = {
  off: 0,
  none: 1,
  minimal: 2,
  low: 3,
  medium: 4,
  high: 5,
  xhigh: 6,
  max: 7,
};

export function thinkingLabel(level: ThinkingLevel): string {
  return LABELS[level] ?? level;
}

/**
 * The levels the catalogue explicitly knows for a model, or null when it has no entry. Callers use
 * this before automatically replacing a saved level: a missing entry must not silently rewrite a
 * preference while the capability request is still in flight or has failed.
 */
export function knownThinkingLevels(
  provider: string | null | undefined,
  model: string | null | undefined,
  capabilities: ModelThinkingMap,
): ThinkingLevel[] | null {
  const levels = provider && model ? capabilities[provider]?.[model] : undefined;
  return levels && levels.length > 0 ? levels : null;
}

/**
 * The levels the selected model actually accepts. A catalogue miss is not an error: the provider's
 * model list still works, and the picker keeps the four levels it had before capabilities existed.
 */
export function thinkingLevelsFor(
  provider: string | null | undefined,
  model: string | null | undefined,
  capabilities: ModelThinkingMap,
): ThinkingLevel[] {
  return knownThinkingLevels(provider, model, capabilities) ?? [...STATIC_THINKING_LEVELS];
}

/**
 * Move an unsupported saved level to the closest supported one: upward where possible (so a model
 * that dropped "medium" while keeping "high" gets more, not less, reasoning), otherwise the highest
 * available level. Off is a special case: if a model cannot turn reasoning off, the first supported
 * level is the least surprising fallback.
 */
export function nearestThinkingLevel(
  current: ThinkingLevel,
  levels: readonly ThinkingLevel[],
): ThinkingLevel {
  if (levels.length === 0 || levels.includes(current)) return current;
  // `off` (omit the field) and `none` (send an explicit none) are the same user choice. When the
  // model supports the other spelling, use it rather than stepping up to Low.
  if (current === "off" || current === "none") {
    if (levels.includes("off")) return "off";
    if (levels.includes("none")) return "none";
    return levels[0];
  }
  const wanted = RANK[current];
  return levels.find((level) => RANK[level] >= wanted) ?? levels[levels.length - 1];
}

/** Ready-made Combo groups for a model's levels. */
export function thinkingGroups(levels: readonly ThinkingLevel[]) {
  return [
    {
      options: levels.map((value) => ({ value, label: thinkingLabel(value) })),
    },
  ];
}
