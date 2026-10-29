import { z } from "zod";
import { thinkingLevelSchema } from "./sessions";

/**
 * Settings and secret status.
 *
 * Rust owns the wire shape; these schemas are pinned by `contract/fixtures/settings.json` and
 * `secret-status.json` on both sides. `Settings` is deliberately open: unknown keys are preserved by
 * the core, so a newer build can add a setting without breaking an older webview.
 */

export const themeSchema = z.enum(["system", "light", "dark"]);
export type Theme = z.infer<typeof themeSchema>;

/**
 * The one global small model used to name sessions. `null` means no explicit choice: generation then
 * uses the suggested model of the first configured provider. The suggestion is never written here
 * silently.
 */
export const titleModelSchema = z.object({
  provider: z.string(),
  model: z.string(),
});
export type TitleModel = z.infer<typeof titleModelSchema>;

export const settingsSchema = z
  .object({
    /** `owner/name` GitHub repositories, in display order. */
    catalogSources: z.array(z.string()),
    theme: themeSchema,
    /** Whether an auto-titled session may be named by a model after its first reply. */
    autoTitles: z.boolean(),
    /** Default reasoning level for a new session. */
    thinkingLevel: thinkingLevelSchema.optional(),
    /** The explicit title model, or null when the suggested one applies. */
    titleModel: titleModelSchema.nullable().optional(),
  })
  .catchall(z.unknown());
export type Settings = z.infer<typeof settingsSchema>;

/**
 * A partial update. Omitted fields keep their stored value; unknown fields are written through, which
 * is how the panel round-trips settings it does not understand.
 *
 * `titleModel: null` cannot be told apart from an omitted field on the Rust side, so choosing Off
 * sends `clearTitleModel: true` instead of a null value.
 */
export const settingsPatchSchema = z
  .object({
    catalogSources: z.array(z.string()).optional(),
    theme: themeSchema.optional(),
    autoTitles: z.boolean().optional(),
    thinkingLevel: thinkingLevelSchema.optional(),
    titleModel: titleModelSchema.optional(),
    clearTitleModel: z.boolean().optional(),
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
