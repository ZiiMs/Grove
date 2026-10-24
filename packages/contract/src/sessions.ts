import { z } from "zod";

/**
 * Session contract: the durable conversation row, its transcript, and the normalized events every
 * harness reduces to. Rust owns the wire shape (`crates/grove-core/src/ipc/types.rs` and
 * `sessions/event.rs`); the golden files in `contract/fixtures/session*.json` are asserted from both
 * sides.
 *
 * Event payload fields stay snake_case (`call_id`, `input_tokens`): they are the shape the adapter
 * contract settles on, and renaming them per DTO would make the event log harness-specific again.
 */

export const sessionStateSchema = z.enum([
  "starting",
  "ready",
  "busy",
  "idle",
  "exited",
  "error",
]);
export type SessionState = z.infer<typeof sessionStateSchema>;

export const roleSchema = z.enum(["user", "assistant", "system"]);
export type Role = z.infer<typeof roleSchema>;

export const blockSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("text"), text: z.string() }),
  z.object({ type: z.literal("thinking"), text: z.string() }),
  z.object({ type: z.literal("code"), language: z.string(), text: z.string() }),
]);
export type Block = z.infer<typeof blockSchema>;

export const toolStatusSchema = z.enum(["ok", "error", "cancelled"]);
export type ToolStatus = z.infer<typeof toolStatusSchema>;

export const decisionSchema = z.enum(["approve", "deny"]);
export type Decision = z.infer<typeof decisionSchema>;

/**
 * Every normalized event kind and its payload, discriminated on `kind`, with the timestamp the core
 * recorded it at. Adding a kind is additive: the router drops kinds it does not know, so an older
 * webview never breaks on a newer core.
 */
export const harnessEventSchema = z.discriminatedUnion("kind", [
  z.object({
    at: z.string(),
    kind: z.literal("session_started"),
    harness: z.string(),
    model: z.string().nullable(),
    resumed: z.boolean(),
  }),
  z.object({ at: z.string(), kind: z.literal("delta"), text: z.string() }),
  z.object({
    at: z.string(),
    kind: z.literal("message"),
    role: roleSchema,
    blocks: z.array(blockSchema),
  }),
  z.object({
    at: z.string(),
    kind: z.literal("tool_call"),
    call_id: z.string(),
    name: z.string(),
    title: z.string(),
  }),
  z.object({
    at: z.string(),
    kind: z.literal("tool_result"),
    call_id: z.string(),
    status: toolStatusSchema,
    summary: z.string(),
  }),
  z.object({
    at: z.string(),
    kind: z.literal("diff"),
    path: z.string(),
    patch: z.string(),
    added: z.number().int(),
    removed: z.number().int(),
  }),
  z.object({
    at: z.string(),
    kind: z.literal("permission_request"),
    request_id: z.string(),
    title: z.string(),
    detail: z.string(),
  }),
  z.object({
    at: z.string(),
    kind: z.literal("permission_resolved"),
    request_id: z.string(),
    decision: decisionSchema,
  }),
  z.object({
    at: z.string(),
    kind: z.literal("usage"),
    input_tokens: z.number().int(),
    output_tokens: z.number().int(),
  }),
  z.object({ at: z.string(), kind: z.literal("status"), state: sessionStateSchema }),
  z.object({
    at: z.string(),
    kind: z.literal("error"),
    message: z.string(),
    retryable: z.boolean(),
  }),
]);
export type HarnessEvent = z.infer<typeof harnessEventSchema>;
export type HarnessEventKind = HarnessEvent["kind"];

export const sessionSchema = z.object({
  id: z.string(),
  harness: z.string(),
  /** Forward-slash working directory the harness runs in. */
  cwd: z.string(),
  model: z.string().nullable(),
  title: z.string(),
  status: sessionStateSchema,
  createdAt: z.string(),
  updatedAt: z.string(),
});
export type Session = z.infer<typeof sessionSchema>;

/** A session list row: the session plus its transcript size. */
export const sessionSummarySchema = sessionSchema.extend({
  eventCount: z.number().int().nonnegative(),
});
export type SessionSummary = z.infer<typeof sessionSummarySchema>;

/** A session and its full transcript, in sequence order. */
export const sessionDetailSchema = sessionSchema.extend({
  events: z.array(harnessEventSchema),
});
export type SessionDetail = z.infer<typeof sessionDetailSchema>;

/** Capability honesty: the UI degrades from these flags, never from the harness name. */
export const capabilitiesSchema = z.object({
  streamingDeltas: z.boolean(),
  toolCalls: z.boolean(),
  diffs: z.boolean(),
  permissions: z.boolean(),
  resume: z.boolean(),
  images: z.boolean(),
  mcp: z.boolean(),
  usage: z.boolean(),
});
export type Capabilities = z.infer<typeof capabilitiesSchema>;

/** A registered harness, for the session picker. */
export const harnessInfoSchema = z.object({
  id: z.string(),
  capabilities: capabilitiesSchema,
});
export type HarnessInfo = z.infer<typeof harnessInfoSchema>;

/** Arguments to `session_create`. */
export const createSessionRequestSchema = z.object({
  harness: z.string(),
  cwd: z.string().nullish(),
  model: z.string().nullish(),
  title: z.string().nullish(),
});
export type CreateSessionRequest = z.infer<typeof createSessionRequestSchema>;
