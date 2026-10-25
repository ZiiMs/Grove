import type { AppInfo } from "@grove/contract";

/**
 * Command surface, one interface per domain. Later tickets add their interface to the intersection in
 * `commands/index.ts` — one line each, so parallel merges stay trivial.
 */
export interface AppCommands {
  app_info: { args: Record<string, never>; result: AppInfo };
  /**
   * Point Grove at the project the user is working in. The session's `cwd` decides it, so the store
   * calls this when a session is created or a tab is activated; the spec graph, the plan mirror, MCP
   * discovery and the project-local skills target all follow. Returns the root now in force.
   */
  app_set_project_root: { args: { path: string }; result: string };
  /** Custom window chrome: the native frame is removed, so the titlebar drives these. */
  app_window_minimize: { args: Record<string, never>; result: null };
  app_window_toggle_maximize: { args: Record<string, never>; result: boolean };
  app_window_is_maximized: { args: Record<string, never>; result: boolean };
  app_window_close: { args: Record<string, never>; result: null };
}
