import type { McpDiscovery, ServerProbe } from "@grove/contract";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { CommandArgs, CommandMap, CommandName } from "../../ipc/commands";
import { BridgeProvider } from "../../ipc/context";
import { createFakeBridge, type FakeBridge } from "../../test-utils/fakeBridge";
import { McpPanel } from "./index";

const discovery: McpDiscovery = {
  servers: [
    {
      name: "filesystem",
      source: ".mcp.json",
      sources: [".mcp.json", "~/.claude.json"],
      transport: "stdio",
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-filesystem"],
      url: null,
    },
    {
      name: "docs",
      source: "~/.claude.json",
      sources: ["~/.claude.json"],
      transport: "http",
      command: null,
      args: [],
      url: "https://mcp.example.com/docs",
    },
  ],
  warnings: [{ source: "~/.codex/config.toml", message: "invalid TOML: expected `=`" }],
};

const connectedProbe: ServerProbe = {
  name: "filesystem",
  source: ".mcp.json",
  transport: "stdio",
  state: "connected",
  tools: [
    { name: "read_file", description: "Read a file from the project" },
    { name: "list_directory", description: null },
  ],
  error: null,
  stderrTail: null,
};

const failedProbe: ServerProbe = {
  name: "docs",
  source: "~/.claude.json",
  transport: "http",
  state: "error",
  tools: [],
  error: "timed out after 10s",
  stderrTail: "docs server starting…\nlisten EADDRINUSE",
};

interface Handlers {
  discovery?: McpDiscovery;
  discoveryError?: { code: string; message: string };
  probe?: ServerProbe;
  probeError?: { code: string; message: string };
}

function createMcpBridge(handlers: Handlers) {
  const calls: { command: string; args: unknown }[] = [];
  const bridge: FakeBridge = createFakeBridge({
    async invoke<K extends CommandName>(
      command: K,
      args?: CommandArgs<K>,
    ): Promise<CommandMap[K]["result"]> {
      calls.push({ command, args });
      switch (command) {
        case "mcp_servers":
          if (handlers.discoveryError) throw handlers.discoveryError;
          return (handlers.discovery ?? discovery) as CommandMap[K]["result"];
        case "mcp_probe":
          if (handlers.probeError) throw handlers.probeError;
          return (handlers.probe ?? connectedProbe) as CommandMap[K]["result"];
        default:
          throw new Error(`FakeBridge has no handler for command: ${command as string}`);
      }
    },
  });
  return { bridge, calls };
}

function renderPanel(bridge: FakeBridge) {
  render(
    <BridgeProvider bridge={bridge}>
      <McpPanel />
    </BridgeProvider>,
  );
}

async function group(source: string) {
  return screen.findByTestId(`mcp-source-${source}`);
}

describe("McpPanel", () => {
  it("lists servers grouped by source and a warning row, without probing anything", async () => {
    const { bridge, calls } = createMcpBridge({});
    renderPanel(bridge);

    const project = await group(".mcp.json");
    expect(within(project).getByText("filesystem")).toBeInTheDocument();

    const claude = await group("~/.claude.json");
    expect(within(claude).getByText("docs")).toBeInTheDocument();

    const codex = await group("~/.codex/config.toml");
    expect(within(codex).getByTestId("mcp-warning")).toHaveTextContent(
      /invalid TOML: expected `=`/,
    );

    // A shared definition shows where else it is declared.
    expect(within(project).getByText(/Also in ~\/.claude.json/)).toBeInTheDocument();

    // Nothing is spawned behind the user's back: mounting the panel only reads the configs.
    expect(calls.map((call) => call.command)).toEqual(["mcp_servers"]);
    expect(bridge.invocations).not.toContain("mcp_probe");
    expect(within(project).getByTestId("mcp-server")).toHaveAttribute("data-state", "idle");
  });

  it("connects on demand and lists the tools the server exposes", async () => {
    const { bridge, calls } = createMcpBridge({});
    renderPanel(bridge);

    const project = await group(".mcp.json");
    const row = within(project).getByTestId("mcp-server");
    await userEvent.click(within(row).getByRole("button", { name: "Connect filesystem" }));

    expect(await within(row).findByTestId("mcp-tools")).toHaveTextContent("read_file");
    expect(within(row).getByText("Read a file from the project")).toBeInTheDocument();
    expect(within(row).getByTestId("mcp-tools")).toHaveTextContent("list_directory");
    expect(row).toHaveAttribute("data-state", "connected");
    expect(calls.filter((call) => call.command === "mcp_probe")).toEqual([
      { command: "mcp_probe", args: { name: "filesystem", source: ".mcp.json" } },
    ]);
  });

  it("shows the error state with the message and the stderr tail", async () => {
    const { bridge } = createMcpBridge({ probe: failedProbe });
    renderPanel(bridge);

    const claude = await group("~/.claude.json");
    const row = within(claude).getByTestId("mcp-server");
    await userEvent.click(within(row).getByRole("button", { name: "Connect docs" }));

    expect(await within(row).findByTestId("mcp-failure")).toHaveTextContent(/timed out after 10s/);
    expect(within(row).getByTestId("mcp-stderr")).toHaveTextContent(/EADDRINUSE/);
    expect(row).toHaveAttribute("data-state", "error");
    expect(within(row).queryByTestId("mcp-tools")).not.toBeInTheDocument();
  });

  it("reports a rejected probe command without losing the list", async () => {
    const { bridge } = createMcpBridge({
      probeError: { code: "not_found", message: "filesystem (.mcp.json)" },
    });
    renderPanel(bridge);

    const project = await group(".mcp.json");
    const row = within(project).getByTestId("mcp-server");
    await userEvent.click(within(row).getByRole("button", { name: "Connect filesystem" }));

    expect(await within(row).findByTestId("mcp-command-error")).toHaveTextContent(
      /filesystem \(\.mcp\.json\)/,
    );
    expect(screen.getByText("docs")).toBeInTheDocument();
  });

  it("explains an empty result instead of showing a blank panel", async () => {
    const { bridge } = createMcpBridge({ discovery: { servers: [], warnings: [] } });
    renderPanel(bridge);

    expect(await screen.findByTestId("mcp-empty")).toHaveTextContent(/No MCP servers/i);
  });

  it("shows a discovery failure with a retry control", async () => {
    const { bridge } = createMcpBridge({
      discoveryError: { code: "io", message: "permission denied" },
    });
    renderPanel(bridge);

    expect(await screen.findByTestId("mcp-list-error")).toHaveTextContent(/permission denied/);
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });

  it("re-reads the configs on Rescan without probing", async () => {
    const { bridge, calls } = createMcpBridge({});
    renderPanel(bridge);

    await group(".mcp.json");
    await userEvent.click(screen.getByRole("button", { name: "Rescan MCP servers" }));

    expect(calls.filter((call) => call.command === "mcp_servers")).toHaveLength(2);
    expect(calls.map((call) => call.command)).not.toContain("mcp_probe");
  });
});
