import {
  parseEventEnvelope,
  parseGitChanged,
  parsePlanChanged,
  parseSpecChanged,
  sessionEventSchema,
  type GitStatus,
  type SessionEvent,
  type Plan,
  type SpecChange,
} from "@grove/contract";
import type { GroveBridge, UnlistenFn } from "./bridge";

export const SESSION_EVENT = "grove://session-event" as const;
export const GIT_CHANGED = "grove://git-changed" as const;
export const PLAN_CHANGED = "grove://plan-changed" as const;
export const SPEC_CHANGED = "grove://spec-changed" as const;

export type SessionEventHandler = (sessionId: string | null, event: SessionEvent) => void;
export type GitChangedHandler = (status: GitStatus) => void;
export type PlanChangedHandler = (sessionId: string | null, plan: Plan) => void;
export type SpecChangedHandler = (change: SpecChange) => void;

/**
 * Typed session event stream. The envelope is parsed once by the contract, then the payload is
 * narrowed to a complete `SessionEvent`: an unknown kind or a malformed payload is dropped here, so no
 * store or panel has to defend against a newer core — the webview's zod gate from
 * `architecture.md` §IPC contract.
 */
export async function subscribeToSessionEvents(
  bridge: GroveBridge,
  handler: SessionEventHandler,
): Promise<UnlistenFn> {
  return bridge.listen(SESSION_EVENT, (raw) => {
    const envelope = parseEventEnvelope(raw);
    if (!envelope) return;
    const parsed = sessionEventSchema.safeParse(envelope.payload);
    if (parsed.success) handler(envelope.sessionId ?? null, parsed.data);
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

/**
 * Subscribe to `grove://spec-changed`: the spec watcher coalesced file changes, or the shell moved
 * the project root. The payload is the relative paths that changed, which is empty for a move — a
 * panel that re-indexes on the event covers both, and never assumes the list is complete.
 */
export async function subscribeToSpecChanged(
  bridge: GroveBridge,
  handler: SpecChangedHandler,
): Promise<UnlistenFn> {
  return bridge.listen(SPEC_CHANGED, (raw) => {
    const envelope = parseSpecChanged(raw);
    if (envelope) handler(envelope.payload);
  });
}
