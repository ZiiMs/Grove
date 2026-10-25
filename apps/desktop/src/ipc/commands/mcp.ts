import type { McpDiscovery, ServerProbe } from "@grove/contract";

/** MCP commands. One interface per domain; `index.ts` joins them. */
export interface McpCommands {
  /** Every server the provider configs declare. Reads files only: it never starts a server. */
  mcp_servers: { args: Record<string, never>; result: McpDiscovery };
  /** Explicitly probe one server; a refused connection comes back as a probe state, not an error. */
  mcp_probe: { args: { name: string; source: string }; result: ServerProbe };
}
