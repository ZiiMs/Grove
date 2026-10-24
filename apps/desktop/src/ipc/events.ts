import {
  harnessEventSchema,
  parseEventEnvelope,
  parseGitChanged,
  parsePlanChanged,
  type EventEnvelope,
  type GitStatus,
  type HarnessEvent,
  type Plan,
} from "@grove/contract";
import type { GroveBridge, UnlistenFn } from "./bridge";

export const SESSION_EVENT = "grove://session-event" as const;
export const GIT_CHANGED = "grove://git-changed" as const;
export const PLAN_CHANGED = "grove://plan-changed" as const;

export type SessionEventHandler = (envelope: EventEnvelope) => void;
export type GitChangedHandler = (status: GitStatus) => void;
export type PlanChangedHandler = (sessionId: string | null, plan: Plan) => void;

/**
 * Subscribe to the session event stream. Malformed payloads and unknown event kinds are dropped here,
 * once, so no store or panel has to defend against a newer core.
 */
export async function subscribeToSessionEvents(
  bridge: GroveBridge,
  handler: SessionEventHandler,
): Promise<UnlistenFn> {
  return bridge.listen(SESSION_EVENT, (raw) => {
    const envelope = parseEventEnvelope(raw);
    if (envelope) handler(envelope);
  });
}

/** Subscribe to `grove://git-changed`, which carries the fresh status after a commit succeeds. */
export async function subscribeToGitChanged(
  bridge: GroveBridge,
  handler: GitChangedHandler,
): Promise<UnlistenFn> {
  return bridge.listen(GIT_CHANGED, (raw) => {
    const envelope = parseGitChanged(raw);
    if (envelope) handler(envelope.payload);
  });
}

/**
 * Subscribe to `grove://plan-changed`, emitted when the core imported an agent's edit to the plan
 * mirror. Malformed payloads are dropped here, like every other event.
 */
export async function subscribeToPlanChanged(
  bridge: GroveBridge,
  handler: PlanChangedHandler,
): Promise<UnlistenFn> {
  return bridge.listen(PLAN_CHANGED, (raw) => {
    const envelope = parsePlanChanged(raw);
    if (envelope) handler(envelope.sessionId ?? null, envelope.payload);
  });
}

export type HarnessEventHandler = (sessionId: string | null, event: HarnessEvent) => void;

/**
 * Typed session event stream: the same subscription, but the payload must be a complete
 * `HarnessEvent` before a store sees it. This is the webview's zod gate from `architecture.md` §IPC
 * contract — an unknown kind is ignored, never fatal.
 */
export async function subscribeToHarnessEvents(
  bridge: GroveBridge,
  handler: HarnessEventHandler,
): Promise<UnlistenFn> {
  return subscribeToSessionEvents(bridge, (envelope) => {
    const parsed = harnessEventSchema.safeParse(envelope.payload);
    if (parsed.success) handler(envelope.sessionId ?? null, parsed.data);
  });
}
