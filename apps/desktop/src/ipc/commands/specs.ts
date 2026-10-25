import type { SpecDoc, SpecGraph } from "@grove/contract";

/**
 * Spec graph commands. `spec_index` roots itself at the core's project root and returns that root in
 * the graph; `spec_read`/`spec_write` take paths relative to it. `expectedHash` is the hash from the
 * read the edit was based on.
 */
export interface SpecsCommands {
  spec_index: { args: Record<string, never>; result: SpecGraph };
  spec_read: { args: { path: string }; result: SpecDoc };
  spec_write: { args: { path: string; body: string; expectedHash: string }; result: SpecDoc };
}
