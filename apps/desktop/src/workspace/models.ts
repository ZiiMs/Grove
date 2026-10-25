import type { Session, SessionState } from "@grove/contract";

/** Presentation for a provider. The backend owns the facts; this is only how a row looks. */
export interface ProviderMeta {
  id: string;
  name: string;
  short: string;
  tint: string;
  models: string[];
}

/**
 * Presentation fallback, so a mark renders before `provider_list` has answered. The catalogue in Rust
 * is authoritative for models, default model and auth; this map only supplies a name, initials, tint
 * and a first guess at the model list. An unknown id degrades to its initials, which is what keeps a
 * fifth vendor working before this file learns about it.
 */
export const PROVIDERS: Record<string, ProviderMeta> = {
  anthropic: {
    id: "anthropic",
    name: "Anthropic",
    short: "AN",
    tint: "#c08a63",
    models: ["claude-sonnet-4-5", "claude-opus-4-1", "claude-haiku-4-5"],
  },
  openai: {
    id: "openai",
    name: "OpenAI",
    short: "OA",
    tint: "#6f9ad1",
    models: ["gpt-5", "gpt-5-mini", "o4-mini"],
  },
  google: {
    id: "google",
    name: "Google",
    short: "GO",
    tint: "#6fbf9a",
    models: ["gemini-2.5-pro", "gemini-2.5-flash"],
  },
  openrouter: {
    id: "openrouter",
    name: "OpenRouter",
    short: "OR",
    tint: "#9d86c9",
    models: ["anthropic/claude-sonnet-4.5", "openai/gpt-5", "google/gemini-2.5-pro"],
  },
  /** Preview/test builds only: the session runtime is real, the model is a script. */
  mock: {
    id: "mock",
    name: "Offline",
    short: "OF",
    tint: "#8a8f98",
    models: ["mock-1"],
  },
};

export function providerMeta(id: string): ProviderMeta {
  return (
    PROVIDERS[id] ?? {
      id,
      name: id,
      short: id.slice(0, 2).toUpperCase(),
      tint: "#8a8f98",
      models: [`${id}-default`],
    }
  );
}

/** The provider a model belongs to, as the design derives it from the picker value. */
export function modelProvider(model: string): string | null {
  for (const provider of Object.values(PROVIDERS)) {
    if (provider.models.includes(model)) return provider.id;
  }
  return null;
}

/**
 * The provider that owns `model`, resolved from the groups the picker is actually showing before the
 * static map. A model that came from the vendor's catalogue is therefore attributed to the right
 * provider even when this file has never heard of it.
 */
export function providerForModel(
  model: string,
  groups: Array<{ provider: ProviderMeta; models: string[] }> = [],
): string | null {
  for (const group of groups) {
    if (group.models.includes(model)) return group.provider.id;
  }
  return modelProvider(model);
}

/** Model groups for the composer's picker: one row per enabled provider. */
export function modelGroupsFor(
  enabled: Record<string, boolean> = {},
  available?: string[],
  modelsByProvider: Record<string, string[]> = {},
): Array<{ provider: ProviderMeta; models: string[] }> {
  const ids = available ?? Object.keys(PROVIDERS);
  return ids
    .filter((id) => enabled[id] !== false)
    .map((id) => ({
      provider: providerMeta(id),
      models: modelsByProvider[id] ?? providerMeta(id).models,
    }));
}

/** The provider a new session should start on: the preferred one when it is usable, else the first. */
export function firstAvailableProvider(
  enabled: Record<string, boolean> = {},
  available?: string[],
  preferred?: string,
): string {
  const ids = available ?? Object.keys(PROVIDERS);
  const usable = (id: string) => enabled[id] !== false;
  if (preferred && ids.includes(preferred) && usable(preferred)) return preferred;
  return ids.find(usable) ?? ids[0] ?? "anthropic";
}

export function defaultModelFor(providerId: string): string {
  return providerMeta(providerId).models[0] ?? `${providerId}-default`;
}

/** What the tree dot and header say. `awaiting` is the agent asking a question, not a status. */
export type SessionVisual = "running" | "waiting" | "failed" | "done" | "idle";

export const SESSION_UI_STATE: Record<SessionVisual, string> = {
  running: "Running",
  waiting: "Awaiting approval",
  failed: "Failed",
  done: "Done",
  idle: "Ready",
};

export function sessionVisual(session: Pick<Session, "status" | "awaiting"> | null): SessionVisual {
  if (!session) return "idle";
  if (session.awaiting) return "waiting";
  return visualForStatus(session.status);
}

function visualForStatus(status: SessionState): SessionVisual {
  switch (status) {
    case "starting":
    case "ready":
    case "busy":
      return "running";
    case "error":
      return "failed";
    case "exited":
      return "done";
    case "idle":
    default:
      return "idle";
  }
}

export function isRunning(session: Pick<Session, "status" | "awaiting"> | null): boolean {
  const visual = sessionVisual(session);
  return visual === "running" || visual === "waiting";
}
