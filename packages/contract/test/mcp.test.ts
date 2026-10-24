import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { mcpDiscoverySchema, serverProbeSchema } from "../src/index";

function fixture(name: string): unknown {
  const path = fileURLToPath(new URL(`../../../contract/fixtures/${name}`, import.meta.url));
  return JSON.parse(readFileSync(path, "utf8"));
}

describe("mcp contract fixtures", () => {
  it("mcp-servers.json matches the zod schema", () => {
    const parsed = mcpDiscoverySchema.parse(fixture("mcp-servers.json"));
    expect(parsed.servers).toHaveLength(2);
    expect(parsed.servers[0].transport).toBe("stdio");
    expect(parsed.servers[0].sources).toEqual([".mcp.json", "~/.claude.json"]);
    expect(parsed.servers[0].args).toContain("@modelcontextprotocol/server-filesystem");
    expect(parsed.servers[1].url).toBe("https://mcp.example.com/linear");
    expect(parsed.warnings[0].source).toBe("opencode.json");
  });

  it("mcp-probe.json matches the zod schema", () => {
    const parsed = serverProbeSchema.parse(fixture("mcp-probe.json"));
    expect(parsed.state).toBe("connected");
    expect(parsed.tools.map((tool) => tool.name)).toEqual(["read_file", "list_directory"]);
    expect(parsed.tools[1].description).toBeNull();
    expect(parsed.error).toBeNull();
    expect(parsed.stderrTail).toBeNull();
  });

  it("mcp-probe-error.json matches the zod schema", () => {
    const parsed = serverProbeSchema.parse(fixture("mcp-probe-error.json"));
    expect(parsed.state).toBe("error");
    expect(parsed.tools).toEqual([]);
    expect(parsed.error).toContain("timed out");
    expect(parsed.stderrTail).toContain("sleeping");
  });

  it("rejects a server with an unknown transport", () => {
    expect(() =>
      mcpDiscoverySchema.parse({
        servers: [
          {
            name: "x",
            source: ".mcp.json",
            sources: [".mcp.json"],
            transport: "carrier-pigeon",
            command: null,
            args: [],
            url: null,
          },
        ],
        warnings: [],
      }),
    ).toThrow();
  });

  it("rejects a probe state the core never produces", () => {
    expect(() =>
      serverProbeSchema.parse({
        name: "x",
        source: ".mcp.json",
        transport: "stdio",
        state: "connecting",
        tools: [],
        error: null,
        stderrTail: null,
      }),
    ).toThrow();
  });
});
