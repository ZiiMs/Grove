import { z } from "zod";

/**
 * Settings, secret status and the harness doctor.
 *
 * Rust owns the wire shape; these schemas are pinned by `contract/fixtures/settings.json`,
 * `secret-status.json` and `doctor.json` on both sides. `Settings` is deliberately open: unknown keys
 * are preserved by the core, so a newer build can add a setting without breaking an older webview.
 */

export const themeSchema = z.enum(["system", "light", "dark"]);
export type Theme = z.infer<typeof themeSchema>;

export const settingsSchema = z
  .object({
    /** Harness id → binary path override; an empty map resolves every harness on PATH. */
    harnessPaths: z.record(z.string()),
    /** `owner/name` GitHub repositories, in display order. */
    catalogSources: z.array(z.string()),
    theme: themeSchema,
  })
  .catchall(z.unknown());
export type Settings = z.infer<typeof settingsSchema>;

/**
 * A partial update. Omitted fields keep their stored value; unknown fields are written through, which
 * is how the panel round-trips settings it does not understand.
 */
export const settingsPatchSchema = z
  .object({
    harnessPaths: z.record(z.string()).optional(),
    catalogSources: z.array(z.string()).optional(),
    theme: themeSchema.optional(),
  })
  .catchall(z.unknown());
export type SettingsPatch = z.infer<typeof settingsPatchSchema>;

/** `unavailable` is distinct from `absent`: a locked keychain is not "the user set no token". */
export const secretStatusSchema = z.enum(["present", "absent", "unavailable"]);
export type SecretStatus = z.infer<typeof secretStatusSchema>;

/** The only secret shape that crosses IPC. There is no command that returns a value. */
export const secretStatusViewSchema = z.object({
  key: z.string(),
  status: secretStatusSchema,
});
export type SecretStatusView = z.infer<typeof secretStatusViewSchema>;

/** One row of the doctor table: installed or not, with the version, path and reason. */
export const harnessHealthSchema = z.object({
  id: z.string(),
  label: z.string(),
  installed: z.boolean(),
  version: z.string().nullable(),
  path: z.string().nullable(),
  detail: z.string().nullable(),
});
export type HarnessHealth = z.infer<typeof harnessHealthSchema>;

/** `node`, `git` and `gh` use the same row shape as a harness. */
export const toolHealthSchema = harnessHealthSchema;
export type ToolHealth = z.infer<typeof toolHealthSchema>;

export const doctorReportSchema = z.object({
  harnesses: z.array(harnessHealthSchema),
  tools: z.array(toolHealthSchema),
  checkedAt: z.string(),
});
export type DoctorReport = z.infer<typeof doctorReportSchema>;
