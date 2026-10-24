import type {
  McpConfigWarning,
  McpDiscovery,
  McpServerView,
  ServerProbe,
} from "@grove/contract";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useBridge } from "../../ipc/context";
import { PanelFrame } from "../PanelFrame";
import "./mcp.css";

type RowState = "idle" | "connecting" | "connected" | "error";

const STATE_LABEL: Record<RowState, string> = {
  idle: "Not probed",
  connecting: "Connecting…",
  connected: "Connected",
  error: "Error",
};

interface ProbeEntry {
  state: RowState;
  probe: ServerProbe | null;
  /** Present when the command itself was rejected (e.g. the definition disappeared from the config). */
  error: string | null;
}

function errorMessage(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string") return message;
  }
  if (error instanceof Error) return error.message;
  return String(error);
}

function rowKey(server: McpServerView): string {
  return `${server.source}::${server.name}`;
}

function invocation(server: McpServerView): string {
  if (server.transport === "http") return server.url ?? "";
  return [server.command ?? "", ...server.args].join(" ").trim();
}

/**
 * Every MCP server the harnesses would use, where each definition comes from, and — when the user asks
 * — whether it connects and what tools it exposes. Discovery only reads config files; a server is
 * spawned only by pressing Connect on its row, never on mount.
 */
export function McpPanel() {
  const bridge = useBridge();
  const [discovery, setDiscovery] = useState<McpDiscovery | null>(null);
  const [listError, setListError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [probes, setProbes] = useState<Record<string, ProbeEntry>>({});

  const load = useCallback(async () => {
    setLoading(true);
    setListError(null);
    try {
      setDiscovery(await bridge.invoke("mcp_servers"));
    } catch (error) {
      setListError(errorMessage(error));
    } finally {
      setLoading(false);
    }
  }, [bridge]);

  useEffect(() => {
    void load();
  }, [load]);

  const connect = useCallback(
    async (server: McpServerView) => {
      const key = rowKey(server);
      setProbes((current) => ({ ...current, [key]: { state: "connecting", probe: null, error: null } }));
      try {
        const probe = await bridge.invoke("mcp_probe", {
          name: server.name,
          source: server.source,
        });
        setProbes((current) => ({
          ...current,
          [key]: {
            state: probe.state === "connected" ? "connected" : "error",
            probe,
            error: null,
          },
        }));
      } catch (error) {
        setProbes((current) => ({
          ...current,
          [key]: { state: "error", probe: null, error: errorMessage(error) },
        }));
      }
    },
    [bridge],
  );

  // One section per config file, servers and warnings together, in discovery order.
  const sections = useMemo(() => {
    const bySource = new Map<string, { servers: McpServerView[]; warnings: McpConfigWarning[] }>();
    const section = (source: string) => {
      const existing = bySource.get(source);
      if (existing) return existing;
      const created = { servers: [] as McpServerView[], warnings: [] as McpConfigWarning[] };
      bySource.set(source, created);
      return created;
    };
    for (const server of discovery?.servers ?? []) section(server.source).servers.push(server);
    for (const warning of discovery?.warnings ?? []) section(warning.source).warnings.push(warning);
    return [...bySource.entries()].map(([source, entries]) => ({ source, ...entries }));
  }, [discovery]);

  return (
    <PanelFrame
      title="MCP"
      subtitle="Every MCP server your harnesses would use. Nothing is started until you press Connect."
      actions={
        <button type="button" className="input" onClick={() => void load()} disabled={loading}>
          Rescan MCP servers
        </button>
      }
    >
      <div className="mcp-panel">
        {listError ? (
          <div className="mcp-banner is-error" data-testid="mcp-list-error">
            <p>Could not read the MCP configs: {listError}</p>
            <button type="button" className="input" onClick={() => void load()}>
              Retry
            </button>
          </div>
        ) : null}

        {loading && !discovery ? <p className="mcp-muted">Reading MCP config…</p> : null}

        {discovery && sections.length === 0 ? (
          <p className="mcp-empty" data-testid="mcp-empty">
            No MCP servers configured in .mcp.json, ~/.claude.json, ~/.codex/config.toml, opencode.json
            or the adapter config.
          </p>
        ) : null}

        {sections.map((section) => (
          <section
            className="mcp-source"
            key={section.source}
            aria-label={section.source}
            data-testid={`mcp-source-${section.source}`}
          >
            <h2 className="mcp-source-title">{section.source}</h2>
            <ul className="mcp-rows">
              {section.warnings.map((warning) => (
                <li
                  className="mcp-warning"
                  data-testid="mcp-warning"
                  key={`warning-${warning.message}`}
                >
                  <span className="mcp-warning-icon" aria-hidden="true">
                    ⚠
                  </span>
                  <span>{warning.message}</span>
                </li>
              ))}
              {section.servers.map((server) => (
                <McpServerRow
                  key={rowKey(server)}
                  server={server}
                  entry={probes[rowKey(server)]}
                  onConnect={() => void connect(server)}
                />
              ))}
            </ul>
          </section>
        ))}
      </div>
    </PanelFrame>
  );
}

function McpServerRow({
  server,
  entry,
  onConnect,
}: {
  server: McpServerView;
  entry: ProbeEntry | undefined;
  onConnect: () => void;
}) {
  const state: RowState = entry?.state ?? "idle";
  const probe = entry?.probe ?? null;
  const shared = server.sources.filter((source) => source !== server.source);

  return (
    <li className="mcp-server" data-testid="mcp-server" data-state={state}>
      <div className="mcp-server-head">
        <span className="mcp-server-name">{server.name}</span>
        <span className="mcp-tag">{server.transport}</span>
        <span className={`mcp-badge is-${state}`} data-testid="mcp-state">
          {STATE_LABEL[state]}
        </span>
        <button
          type="button"
          className="input mcp-connect"
          aria-label={`Connect ${server.name}`}
          onClick={onConnect}
          disabled={state === "connecting"}
        >
          {state === "connecting" ? "Connecting…" : "Connect"}
        </button>
      </div>

      <p className="mcp-invocation">{invocation(server)}</p>
      {shared.length > 0 ? (
        <p className="mcp-shared">Also in {shared.join(", ")}</p>
      ) : null}

      {state === "connected" && probe ? (
        probe.tools.length > 0 ? (
          <ul className="mcp-tools" data-testid="mcp-tools">
            {probe.tools.map((tool) => (
              <li className="mcp-tool" key={tool.name}>
                <span className="mcp-tool-name">{tool.name}</span>
                {tool.description ? (
                  <span className="mcp-tool-description">{tool.description}</span>
                ) : null}
              </li>
            ))}
          </ul>
        ) : (
          <p className="mcp-muted" data-testid="mcp-tools">
            Connected, but the server advertises no tools.
          </p>
        )
      ) : null}

      {state === "error" && probe ? (
        <div className="mcp-banner is-error" data-testid="mcp-failure">
          <p>{probe.error ?? "The server did not answer."}</p>
          {probe.stderrTail ? (
            <pre className="mcp-stderr" data-testid="mcp-stderr">
              {probe.stderrTail}
            </pre>
          ) : null}
        </div>
      ) : null}

      {entry?.error ? (
        <p className="mcp-banner is-error" data-testid="mcp-command-error">
          {entry.error}
        </p>
      ) : null}
    </li>
  );
}
