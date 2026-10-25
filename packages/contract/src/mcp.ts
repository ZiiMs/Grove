import { z } from "zod";

/**
 * MCP contract: every server definition Grove finds in the harness config files, and the result of one
 * explicit probe. Rust owns the wire shape; these schemas are pinned by
 * `contract/fixtures/mcp-servers.json` and `contract/fixtures/mcp-probe*.json` on both sides.
 */

export const mcpTransportSchema = z.enum(["stdio", "http"]);
export type McpTransport = z.infer<typeof mcpTransportSchema>;

export const mcpServerSchema = z.object({
  name: z.string(),
  /** The config file this row is grouped under, e.g. `.mcp.json` or `~/.claude.json`. */
  source: z.string(),
  /** Every config file that defines this same server; `source` is always `sources[0]`. */
  sources: z.array(z.string()),
  transport: mcpTransportSchema,
  /** The executable for a stdio server; null for http. */
  command: z.string().nullable(),
  /** Arguments for a stdio server; empty for http. */
  args: z.array(z.string()),
  /** The endpoint for an http server; null for stdio. */
  url: z.string().nullable(),
});
export type McpServerView = z.infer<typeof mcpServerSchema>;

/** A config file the core could not read or parse, or a definition it could not normalise. */
export const mcpConfigWarningSchema = z.object({
  source: z.string(),
  message: z.string(),
});
export type McpConfigWarning = z.infer<typeof mcpConfigWarningSchema>;

/** What `mcp_servers` returns. An empty list with no warnings means nothing is configured. */
export const mcpDiscoverySchema = z.object({
  servers: z.array(mcpServerSchema),
  warnings: z.array(mcpConfigWarningSchema),
});
export type McpDiscovery = z.infer<typeof mcpDiscoverySchema>;

export const probeStateSchema = z.enum(["connected", "error"]);
export type ProbeState = z.infer<typeof probeStateSchema>;

export const mcpToolSchema = z.object({
  name: z.string(),
  description: z.string().nullable(),
});
export type McpTool = z.infer<typeof mcpToolSchema>;

/**
 * The result of one explicit probe. A refused connection is this shape with `state: "error"`, not a
 * command failure, so the panel always has an error message and a stderr tail to show.
 */
export const serverProbeSchema = z.object({
  name: z.string(),
  source: z.string(),
  transport: mcpTransportSchema,
  state: probeStateSchema,
  tools: z.array(mcpToolSchema),
  error: z.string().nullable(),
  stderrTail: z.string().nullable(),
});
export type ServerProbe = z.infer<typeof serverProbeSchema>;
