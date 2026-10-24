import { parseEventEnvelope, type EventEnvelope } from "@grove/contract";
import type { GroveBridge, UnlistenFn } from "./bridge";

export const SESSION_EVENT = "grove://session-event" as const;

export type SessionEventHandler = (envelope: EventEnvelope) => void;

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
