import type { AppInfo } from "@grove/contract";

/**
 * Command surface, one interface per domain. Later tickets add their interface to the intersection in
 * `commands/index.ts` — one line each, so parallel merges stay trivial.
 */
export interface AppCommands {
  app_info: { args: Record<string, never>; result: AppInfo };
}
