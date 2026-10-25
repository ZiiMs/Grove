import { z } from "zod";

/**
 * Core IPC contract: the envelope every event rides in, the error shape every command returns, and the
 * app's header identity. Rust owns the wire shape; these schemas must accept exactly what it serialises,
 * which is proven from both sides by the golden files in `contract/fixtures/`.
 *
 * Domain schemas live in their own module (`sessions.ts`, `specs.ts`, …) and are re-exported from
 * `index.ts`, so parallel features never edit this file.
 */

export const errorCodeSchema = z.enum([
  "conflict",
  "not_found",
  "invalid",
  "unavailable",
  "io",
]);
export type ErrorCode = z.infer<typeof errorCodeSchema>;

export const ipcErrorSchema = z.object({
  code: errorCodeSchema,
  message: z.string(),
  detail: z.string().optional(),
});
export type IpcError = z.infer<typeof ipcErrorSchema>;

export const appInfoSchema = z.object({
  name: z.string(),
  version: z.string(),
  appDataDir: z.string(),
  platform: z.string(),
});
export type AppInfo = z.infer<typeof appInfoSchema>;

/**
 * Normalized harness event kinds. Adding one is additive: the router ignores kinds it does not know,
 * so an older webview never breaks on a newer core.
 */
export const eventKinds = [
  "session_started",
  "delta",
  "message",
  "tool_call",
  "tool_result",
  "diff",
  "permission_request",
  "permission_resolved",
  "question_request",
  "question_resolved",
  "usage",
  "status",
  "error",
  "context_refreshed",
] as const;
export type EventKind = (typeof eventKinds)[number];

export const knownEventKinds: ReadonlySet<string> = new Set(eventKinds);

/** `payload` stays loose here; each event kind narrows it where it is consumed. */
export const eventEnvelopeSchema = z.object({
  sessionId: z.string().optional(),
  payload: z
    .object({
      kind: z.string(),
      at: z.string().optional(),
    })
    .passthrough(),
});
export type EventEnvelope = z.infer<typeof eventEnvelopeSchema>;

export function parseEventEnvelope(raw: unknown): EventEnvelope | null {
  const parsed = eventEnvelopeSchema.safeParse(raw);
  if (!parsed.success) return null;
  if (!knownEventKinds.has(parsed.data.payload.kind)) return null;
  return parsed.data;
}
