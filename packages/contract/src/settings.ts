import { z } from "zod";

/**
 * Settings and secret status.
 *
 * Rust owns the wire shape; these schemas are pinned by `contract/fixtures/settings.json` and
 * `secret-status.json` on both sides. `Settings` is deliberately open: unknown keys are preserved by
 * the core, so a newer build can add a setting without breaking an older webview.
 */

export const themeSchema = z.enum(["system", "light", "dark"]);
export type Theme = z.infer<typeof themeSchema>;

export const settingsSchema = z
  .object({
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
