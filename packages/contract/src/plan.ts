import { z } from "zod";

/**
 * Plan contract: one versioned TODO plan per session, edited by the human in the Plan panel and by
 * the agent as `.grove/sessions/<id>/plan.md` inside the session's project.
 *
 * Rust (`grove_core::plan`) owns the wire shape; these schemas must accept exactly what it
 * serialises. Both sides are pinned by `contract/fixtures/plan*.json` — a Rust test serialises a
 * sample per DTO and this package parses the same file.
 *
 * `version` starts at 0 (no plan yet) and moves on every accepted write, human or agent, so a save
 * that carries a stale `expectedVersion` is a `conflict` and the panel reloads.
 */

export const planSchema = z.object({
  sessionId: z.string(),
  markdown: z.string(),
  /** 0 when no plan has been written yet. */
  version: z.number().int().nonnegative(),
  /** Display form (forward slashes) of the mirror file the agent edits. */
  mirrorPath: z.string(),
});
export type Plan = z.infer<typeof planSchema>;

/** The arguments to `plan_set`: the new markdown plus the version it was based on. */
export const planUpdateSchema = z.object({
  markdown: z.string(),
  expectedVersion: z.number().int().nonnegative(),
});
export type PlanUpdate = z.infer<typeof planUpdateSchema>;

/** `grove://plan-changed` carries the imported plan, after the agent changed the mirror. */
export const planChangedSchema = z.object({
  sessionId: z.string().optional(),
  payload: planSchema,
});
export type PlanChangedEvent = z.infer<typeof planChangedSchema>;

/** Validate an incoming `grove://plan-changed` payload; malformed data is dropped, not thrown. */
export function parsePlanChanged(raw: unknown): PlanChangedEvent | null {
  const parsed = planChangedSchema.safeParse(raw);
  return parsed.success ? parsed.data : null;
}
