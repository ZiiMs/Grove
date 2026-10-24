import { z } from "zod";

/**
 * Spec graph contract. The core indexes repository markdown whose YAML frontmatter carries `id` and
 * `type`; these schemas mirror the Rust DTOs in `crates/grove-core/src/specgraph/mod.rs` and are
 * pinned from both sides by `contract/fixtures/spec*.json`.
 *
 * Paths are relative to the graph root and always forward-slash, so the same payload validates on
 * Windows and POSIX.
 */

export const specNodeSchema = z.object({
  id: z.string(),
  type: z.string(),
  title: z.string().optional(),
  status: z.string().optional(),
  path: z.string(),
  parent: z.string().optional(),
  dependsOn: z.array(z.string()),
  references: z.array(z.string()),
  implements: z.array(z.string()),
  covers: z.array(z.string()),
});
export type SpecNode = z.infer<typeof specNodeSchema>;

export const specDanglingLinkSchema = z.object({
  from: z.string(),
  path: z.string(),
  field: z.string(),
  target: z.string(),
});
export type SpecDanglingLink = z.infer<typeof specDanglingLinkSchema>;

export const specDuplicateIdSchema = z.object({
  id: z.string(),
  paths: z.array(z.string()),
});
export type SpecDuplicateId = z.infer<typeof specDuplicateIdSchema>;

export const specParentCycleSchema = z.object({
  ids: z.array(z.string()),
  paths: z.array(z.string()),
});
export type SpecParentCycle = z.infer<typeof specParentCycleSchema>;

export const specValidationSchema = z.object({
  danglingLinks: z.array(specDanglingLinkSchema),
  duplicateIds: z.array(specDuplicateIdSchema),
  parentCycles: z.array(specParentCycleSchema),
});
export type SpecValidation = z.infer<typeof specValidationSchema>;

export const specGraphSchema = z.object({
  root: z.string(),
  nodes: z.array(specNodeSchema),
  validation: specValidationSchema,
});
export type SpecGraph = z.infer<typeof specGraphSchema>;

export const specDocSchema = z.object({
  node: specNodeSchema,
  body: z.string(),
  hash: z.string(),
});
export type SpecDoc = z.infer<typeof specDocSchema>;

/** `grove://spec-changed` payload: relative paths of the spec files that changed. */
export const specChangeSchema = z.object({
  paths: z.array(z.string()),
});
export type SpecChange = z.infer<typeof specChangeSchema>;
