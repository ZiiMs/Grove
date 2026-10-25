import { z } from "zod";
import { contextDocumentSchema } from "./sessions";

/**
 * Session knowledge contract: which specs a session carries, and what its provider is given.
 *
 * A pin is a durable fact about a session (the `pins` table), not a copy of the spec: the id is the
 * stable handle, and the title is resolved from the spec graph at read time. The documents are the
 * result of applying the character budgets, so the header can show exactly what a start would send
 * without re-reading the bodies.
 *
 * Rust (`grove_core::sessions::context`) owns the wire shape; `contract/fixtures/session-context.json`
 * pins both sides.
 */

/** The character budgets a session's context is assembled under. */
export const contextLimitsSchema = z.object({
  perDocChars: z.number().int().positive(),
  totalChars: z.number().int().positive(),
});
export type ContextLimits = z.infer<typeof contextLimitsSchema>;

/** One spec pinned to a session, with the title the session header shows. */
export const contextPinSchema = z.object({
  specId: z.string(),
  title: z.string(),
});
export type ContextPin = z.infer<typeof contextPinSchema>;

/**
 * What a session carries right now: its pins, the documents those pins and the plan produce, and the
 * budgets they were assembled under. `planVersion` is 0 when the session has no plan yet.
 */
export const sessionContextSchema = z.object({
  sessionId: z.string(),
  provider: z.string(),
  pins: z.array(contextPinSchema),
  documents: z.array(contextDocumentSchema),
  totalChars: z.number().int().nonnegative(),
  limits: contextLimitsSchema,
  planVersion: z.number().int().nonnegative(),
});
export type SessionContext = z.infer<typeof sessionContextSchema>;

/** `true` means pin, `false` means unpin. Unknown spec ids are rejected by the core. */
export const sessionPinRequestSchema = z.object({
  id: z.string(),
  specId: z.string(),
  pinned: z.boolean(),
});
export type SessionPinRequest = z.infer<typeof sessionPinRequestSchema>;
