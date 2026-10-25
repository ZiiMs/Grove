import { z } from "zod";

/**
 * Skill catalog contract: the browse list, the preview pane, install destinations, and the install
 * result. Rust owns the wire shape; these schemas are pinned by `contract/fixtures/skill*.json` on
 * both sides.
 */

export const catalogEntrySchema = z.object({
  /** Stable key: `<source>::<repo-relative path>`. */
  id: z.string(),
  /** Directory name, e.g. `pdf`. */
  name: z.string(),
  /** `owner/name`. */
  source: z.string(),
  /** Repository-relative directory path, e.g. `skills/pdf`. */
  path: z.string(),
});
export type CatalogEntry = z.infer<typeof catalogEntrySchema>;

export const skillCatalogSchema = z.object({
  entries: z.array(catalogEntrySchema),
  /** RFC 3339; null only if the core has never completed a fetch (it errors instead). */
  fetchedAt: z.string().nullable(),
  /** True when every entry came from the disk cache. */
  fromCache: z.boolean(),
  /** True when the network failed and a stale cache kept the panel usable. */
  offline: z.boolean(),
});
export type SkillCatalog = z.infer<typeof skillCatalogSchema>;

export const skillPreviewSchema = z.object({
  entry: catalogEntrySchema,
  /** Flat `key: value` frontmatter; nested YAML is out of scope. */
  frontmatter: z.record(z.string()),
  body: z.string(),
});
export type SkillPreview = z.infer<typeof skillPreviewSchema>;

export const skillTargetSchema = z.object({
  /** `claude` | `codex` | `pi` | `opencode` | `agents`. */
  id: z.string(),
  label: z.string(),
  /** Forward-slash display path. */
  path: z.string(),
  exists: z.boolean(),
});
export type SkillTarget = z.infer<typeof skillTargetSchema>;

export const skillTargetsSchema = z.array(skillTargetSchema);

export const installResultSchema = z.object({
  skill: z.string(),
  source: z.string(),
  target: z.string(),
  /** Forward-slash path of the installed folder. */
  path: z.string(),
  /** False for a new install, true when an existing folder was replaced. */
  overwritten: z.boolean(),
  installedAt: z.string(),
});
export type InstallResult = z.infer<typeof installResultSchema>;

/** One recorded installation, so the catalog can mark a skill installed and offer removal. */
export const installRecordSchema = z.object({
  skill: z.string(),
  source: z.string(),
  target: z.string(),
  /** Forward-slash path of the installed folder. */
  path: z.string(),
  installedAt: z.string(),
});
export type InstallRecord = z.infer<typeof installRecordSchema>;
