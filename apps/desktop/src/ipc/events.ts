import {
  parseEventEnvelope,
  parseGitChanged,
  type EventEnvelope,
  type GitStatus,
} from "@grove/contract";
import type { GroveBridge, UnlistenFn } from "./bridge";

export const SESSION_EVENT = "grove://session-event" as const;
export const GIT_CHANGED = "grove://git-changed" as const;

export type SessionEventHandler = (envelope: EventEnvelope) => void;
export type GitChangedHandler = (status: GitStatus) => void;

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
