import { z } from "zod";

/**
 * Session contract: the durable conversation row, its transcript, and the normalized events every
 * provider reduces to. Rust owns the wire shape (`crates/grove-core/src/ipc/types.rs` and
 * `sessions/event.rs`); the golden files in `contract/fixtures/session*.json` are asserted from both
 * sides.
 *
 * Event payload fields stay snake_case (`call_id`, `input_tokens`): they are the shape the adapter
 * contract settles on, and renaming them per DTO would make the event log provider-specific again.
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

/** Who wrote a session's title. Only `auto` still carrying the temporary title may be replaced. */
export const titleSourceSchema = z.enum(["auto", "generated", "manual"]);
export type TitleSource = z.infer<typeof titleSourceSchema>;

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
 * How much hidden reasoning to ask a provider for. The selectable subset is model-specific: the
 * composer reads it from the core's models.dev capability map. `off` omits the vendor field.
 */
export const thinkingLevelSchema = z.enum([
  "off",
  "none",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
]);
export type ThinkingLevel = z.infer<typeof thinkingLevelSchema>;

/** One option in a structured question. */
export const questionOptionSchema = z.object({
  label: z.string(),
  description: z.string().default(""),
});
export type QuestionOption = z.infer<typeof questionOptionSchema>;

/** One question the agent pauses to ask. */
export const questionSchema = z.object({
  id: z.string(),
  title: z.string(),
  hint: z.string().default(""),
  multi: z.boolean().default(false),
  other: z.boolean().default(true),
  options: z.array(questionOptionSchema),
});
export type Question = z.infer<typeof questionSchema>;

/** One selected option and the optional note the user attached to it. */
export const questionChoiceSchema = z.object({
  label: z.string(),
  note: z.string().optional(),
});
export type QuestionChoice = z.infer<typeof questionChoiceSchema>;

/** The answer to one question. Empty `answers` means it was cancelled or expired. */
export const questionAnswerSchema = z.object({
  question_id: z.string(),
  answers: z.array(questionChoiceSchema),
});
export type QuestionAnswer = z.infer<typeof questionAnswerSchema>;

/**
 * Why a session's context documents were assembled and handed to the provider: a new session, a
 * resume, an explicit refresh, or a switch to another provider.
 */
export const contextReasonSchema = z.enum(["start", "resume", "refresh", "provider_switch"]);
export type ContextReason = z.infer<typeof contextReasonSchema>;

/**
 * One document in a session's context, as the transcript records it. The body is deliberately absent:
 * the log keeps what the provider was *given* (id, title, size, truncation), not a second copy of it.
 */
export const contextDocumentSchema = z.object({
  /** The spec id, or `plan` for the session's TODO plan. */
  id: z.string(),
  title: z.string(),
  /** Characters the document contributes, truncation marker included. */
  chars: z.number().int().nonnegative(),
  /** Whether the per-document budget cut it short. */
  truncated: z.boolean(),
});
export type ContextDocument = z.infer<typeof contextDocumentSchema>;

/**
 * Every normalized event kind and its payload, discriminated on `kind`, with the timestamp the core
 * recorded it at. Adding a kind is additive: the router drops kinds it does not know, so an older
 * webview never breaks on a newer core.
 */
export const sessionEventSchema = z.discriminatedUnion("kind", [
  z.object({
    at: z.string(),
    kind: z.literal("session_started"),
    provider: z.string(),
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
    kind: z.literal("question_request"),
    request_id: z.string(),
    questions: z.array(questionSchema),
  }),
  z.object({
    at: z.string(),
    kind: z.literal("question_resolved"),
    request_id: z.string(),
    answers: z.array(questionAnswerSchema),
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
    kind: z.literal("title_changed"),
    title: z.string(),
    source: titleSourceSchema,
  }),
  z.object({
    at: z.string(),
    kind: z.literal("error"),
    message: z.string(),
    retryable: z.boolean(),
  }),
  z.object({
    at: z.string(),
    kind: z.literal("context_refreshed"),
    reason: contextReasonSchema,
    /** The provider that received the documents. */
    provider: z.string(),
    documents: z.array(contextDocumentSchema),
    /** One human-readable line the transcript renders as a system note. */
    note: z.string(),
  }),
]);
export type SessionEvent = z.infer<typeof sessionEventSchema>;
export type SessionEventKind = SessionEvent["kind"];

export const sessionSchema = z.object({
  id: z.string(),
  provider: z.string(),
  /** Forward-slash working directory the provider runs in. */
  cwd: z.string(),
  model: z.string().nullable(),
  title: z.string(),
  /** Who wrote the title: the user, the first-message placeholder, or a model. */
  titleSource: titleSourceSchema,
  status: sessionStateSchema,
  /** The workspace project this session belongs to, or null for a loose chat. */
  projectId: z.string().nullable(),
  /** True while a permission request is waiting for the user. */
  awaiting: z.boolean(),
  /** When the session was archived, or null while it is active. Archive keeps the transcript. */
  archivedAt: z.string().nullable(),
  createdAt: z.string(),
  updatedAt: z.string(),
});
export type Session = z.infer<typeof sessionSchema>;

/** How a delegated worker ended: `running` until the parent's tool result arrives. */
export const subagentStatusSchema = z.enum(["running", "ok", "error", "cancelled"]);
export const subagentSummarySchema = z.object({
  /** The parent's tool-call id; also the prefix on the worker's forwarded transcript rows. */
  callId: z.string(),
  description: z.string(),
  status: subagentStatusSchema,
  /** The stored tool-result summary; the full report only goes to the parent model. */
  summary: z.string().nullable().default(null),
  startedAt: z.string(),
  finishedAt: z.string().nullable().default(null),
});
export type SubagentSummary = z.infer<typeof subagentSummarySchema>;

/** A session list row: the session plus its transcript size and delegations. */
export const sessionSummarySchema = sessionSchema.extend({
  eventCount: z.number().int().nonnegative(),
  /** Workers this session delegated to, oldest first. */
  subagents: z.array(subagentSummarySchema).default([]),
});
export type SessionSummary = z.infer<typeof sessionSummarySchema>;

/** A session and its full transcript, in sequence order. */
export const sessionDetailSchema = sessionSchema.extend({
  /** Reasoning level the runtime will use on the next turn; absent means `off`. */
  thinkingLevel: thinkingLevelSchema.optional(),
  events: z.array(sessionEventSchema),
});
export type SessionDetail = z.infer<typeof sessionDetailSchema>;

/** Capability honesty: the UI degrades from these flags, never from the provider name. */
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

/**
 * A registered provider: everything the picker and the settings pane read. Rust's `ProviderSpec` is
 * the source of truth, so a fifth vendor appears here without a second list in the UI.
 */
export const providerSpecSchema = z.object({
  id: z.string(),
  name: z.string(),
  /** Two-letter mark for the row and the title bar. */
  short: z.string(),
  /** Brand tint for the mark, as a CSS colour. */
  tint: z.string(),
  /** How the vendor's HTTP API is shaped. */
  wire: z.enum(["anthropic", "open_ai", "google"]),
  baseUrl: z.string(),
  auth: z.enum(["api_key", "sign_in"]),
  /** Where the user creates a key. Opened in the browser from Settings. */
  keyUrl: z.string(),
  /** The vendor's own environment variable, shown as a fallback hint. */
  envVar: z.string(),
  models: z.array(z.string()),
  defaultModel: z.string(),
  capabilities: capabilitiesSchema,
});
export type ProviderSpec = z.infer<typeof providerSpecSchema>;

/**
 * The settings row for one provider: whether a credential is configured, where it was found, and why
 * it is unusable when it is. Never carries a key value.
 */
export const providerStatusSchema = providerSpecSchema
  .omit({ wire: true, baseUrl: true, auth: true, capabilities: true })
  .extend({
    method: z.enum(["api_key", "sign_in"]),
    configured: z.boolean(),
    source: z.enum(["keychain", "environment"]).nullable(),
    /** Why a key is unusable, when it is. */
    detail: z.string().nullable(),
  });
export type ProviderStatus = z.infer<typeof providerStatusSchema>;

/**
 * Whether a provider would start: is a credential configured, and which model would it use. The
 * settings pane reads this; there is no binary to detect any more.
 */
export const detectionSchema = z.object({
  installed: z.boolean(),
  method: z.string(),
  defaultModel: z.string(),
  detail: z.string().nullable(),
});
export type Detection = z.infer<typeof detectionSchema>;

/**
 * Arguments to `session_create`.
 *
 * `config` is provider config from settings — the documented key is `maxSteps`, the tool-loop
 * ceiling — so it stays untyped here rather than inventing a second shape per provider. It mirrors
 * Rust's `CreateSessionRequest.config: Option<Value>` exactly, which is what
 * `contract/fixtures/create-session-request.json` pins.
 */
export const createSessionRequestSchema = z.object({
  provider: z.string(),
  cwd: z.string().nullish(),
  model: z.string().nullish(),
  title: z.string().nullish(),
  /** The project the session belongs to. `cwd` then falls back to its worktree/project path. */
  projectId: z.string().nullish(),
  /** A worktree checkout to run in; only meaningful with `projectId`. */
  worktreePath: z.string().nullish(),
  config: z.unknown().nullish(),
});
export type CreateSessionRequest = z.infer<typeof createSessionRequestSchema>;
