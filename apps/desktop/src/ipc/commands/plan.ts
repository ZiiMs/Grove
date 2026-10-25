import type { Plan, PlanUpdate } from "@grove/contract";

/**
 * Plan command surface. The dual-edit rules live in the core; `plan_get` re-checks the mirror so an
 * agent's edit is imported before the panel paints, and `plan_set` presents the version it was
 * based on so a stale save comes back as a `conflict`.
 */
export interface PlanCommands {
  plan_get: { args: { id: string }; result: Plan };
  plan_set: { args: { id: string; update: PlanUpdate }; result: Plan };
}
