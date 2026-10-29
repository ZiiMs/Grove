import { z } from "zod";

/**
 * Provider plan usage, outside the session transcript. Only providers with a metered plan expose
 * this (OpenCode Go today); the command returns `null` for every other provider.
 *
 * The vendor reports **used** percent; `remainingPercent` is the inverse, because that is what the
 * panel shows.
 */
export const planUsageWindowSchema = z.object({
  /** `rolling`, `weekly` or `monthly`. */
  id: z.string(),
  /** The display label, e.g. "5-hour" or "Monthly". */
  label: z.string(),
  usedPercent: z.number().int().min(0).max(100),
  remainingPercent: z.number().int().min(0).max(100),
  /** `ok` while the window has budget, `rate-limited` once it is spent. */
  status: z.enum(["ok", "rate-limited"]),
  /** RFC3339 instant when the window resets. */
  resetsAt: z.string(),
});
export type PlanUsageWindow = z.infer<typeof planUsageWindowSchema>;

export const planUsageSchema = z.object({
  provider: z.string(),
  windows: z.array(planUsageWindowSchema),
});
export type PlanUsage = z.infer<typeof planUsageSchema>;

/** `null` means the provider has no plan endpoint, not that the request failed. */
export const planUsageResultSchema = planUsageSchema.nullable();
