import type { McpDiscovery, ServerProbe } from "@grove/contract";
import { useEffect, useState } from "react";
import { useBridge } from "../ipc/context";
import { errorText } from "../ipc/errors";
import { useUiStore } from "../state/uiStore";
import { Icon } from "../workspace/Icon";

export function McpPane() {
  const bridge = useBridge();
  const notify = useUiStore((state) => state.notify);
  const [discovery, setDiscovery] = useState<McpDiscovery | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [probing, setProbing] = useState<string | null>(null);
  const [probes, setProbes] = useState<Record<string, ServerProbe>>({});

  useEffect(() => {
    let cancelled = false;
    void bridge
      .invoke("mcp_servers")
      .then((next) => {
        if (!cancelled) setDiscovery(next);
      })
      .catch((err) => {
        if (!cancelled) setError(errorText(err));
      });
    return () => {
      cancelled = true;
    };
  }, [bridge]);

  const probe = async (name: string, source: string) => {
    const key = `${name}::${source}`;
    setProbing(key);
    try {
      const result = await bridge.invoke("mcp_probe", { name, source });
      setProbes((current) => ({ ...current, [key]: result }));
      notify(
        result.state === "connected"
          ? `${name} connected · ${result.tools.length} tools`
          : `${name} could not connect`,
        result.state === "connected" ? "ok" : "error",
      );
    } catch (err) {
      notify(errorText(err), "error");
    } finally {
      setProbing(null);
    }
  };

  return (
    <div className="sm-pane-scroll">
      <div className="sm-pane-head">
        <h3>MCP servers</h3>
        <p>
          Every server the provider configs declare. Grove reads the files only; probing starts a
          server explicitly, and a refused connection is reported here rather than failing the app.
        </p>
      </div>

      {error ? <p className="field-err">{error}</p> : null}
      {!discovery && !error ? <p className="git-empty">Reading provider configs…</p> : null}
      {discovery && discovery.servers.length === 0 ? (
        <p className="git-empty">No MCP servers are configured for this project or your providers.</p>
      ) : null}

      <div className="mcp-list">
        {discovery?.servers.map((server) => {
          const key = `${server.name}::${server.source}`;
          const result = probes[key];
          return (
            <div className="mcp-row" key={key}>
              <div className="mcp-main">
                <span className="mcp-name">{server.name}</span>
                <span className="mcp-meta">
                  {server.source} · {server.transport}
                  {server.command ? ` · ${server.command}` : ""}
                  {server.url ? ` · ${server.url}` : ""}
                </span>
                {result ? (
                  <div className="mcp-probe" data-state={result.state}>
                    <span>
                      {result.state === "connected"
                        ? `${result.tools.length} tool${result.tools.length === 1 ? "" : "s"}`
                        : result.error ?? "connection failed"}
                    </span>
                    {result.tools.length ? (
                      <span className="mcp-tools">
                        {result.tools.map((tool) => (
                          <span className="sk-tag" key={tool.name} title={tool.description ?? undefined}>
                            {tool.name}
                          </span>
                        ))}
                      </span>
                    ) : null}
                    {result.stderrTail ? <pre className="mcp-stderr">{result.stderrTail}</pre> : null}
                  </div>
                ) : null}
              </div>
              <button
                className="btn-ghost sm"
                type="button"
                disabled={probing === key}
                onClick={() => void probe(server.name, server.source)}
              >
                <Icon name="play" size="sm" />
                {probing === key ? "Probing…" : "Probe"}
              </button>
            </div>
          );
        })}
      </div>

      {discovery?.warnings.length ? (
        <div className="mcp-warnings">
          {discovery.warnings.map((warning) => (
            <p key={`${warning.source}-${warning.message}`}>
              <Icon name="alert" size="xs" /> {warning.source}: {warning.message}
            </p>
          ))}
        </div>
      ) : null}
    </div>
  );
}
