import type { SpecNode } from "@grove/contract";
import { useBridge } from "../ipc/context";
import { errorText } from "../ipc/errors";
import { useContextStore } from "../state/contextStore";
import { useSessionStore } from "../state/sessionStore";
import { useUiStore } from "../state/uiStore";
import { isRunning } from "../workspace/models";
import { Icon } from "../workspace/Icon";

function specTitle(node: SpecNode): string {
  return node.title?.trim() || node.id;
}

function statusLabel(node: SpecNode): string {
  return node.status?.trim() || node.type.replace(/_/g, " ");
}

function excerpt(body: string): string {
  const text = body
    .split("\n")
    .filter((line) => !line.trim().startsWith("#"))
    .join(" ")
    .replace(/\s+/g, " ")
    .trim();
  return text.length > 220 ? `${text.slice(0, 217)}…` : text;
}

/** The Spec tab: the project's graph, with a read-only peek and attach-to-session. */
export function SpecPanel() {
  const bridge = useBridge();
  const specs = useContextStore((state) => state.specs);
  const specError = useContextStore((state) => state.specError);
  const specDoc = useContextStore((state) => state.specDoc);
  const specDocPath = useContextStore((state) => state.specDocPath);
  const loadSpecDoc = useContextStore((state) => state.loadSpecDoc);
  const closeSpecDoc = useContextStore((state) => state.closeSpecDoc);
  const context = useSessionStore((state) => state.context);
  const activeId = useSessionStore((state) => state.activeId);
  const detail = useSessionStore((state) => state.detail);
  const applyContext = useSessionStore((state) => state.applyContext);
  const refreshContext = useSessionStore((state) => state.refreshContext);
  const notify = useUiStore((state) => state.notify);

  const nodes = specs?.nodes ?? [];
  const accepted = nodes.filter((node) => node.status?.toLowerCase() === "accepted").length;
  const selected = specDoc?.node ?? nodes.find((node) => node.path === specDocPath) ?? null;
  const pinned = new Set(context?.pins.map((pin) => pin.specId) ?? []);

  const attach = async (node: SpecNode) => {
    if (!activeId) {
      notify("Open a session before attaching a spec", "error");
      return;
    }
    try {
      const next = await bridge.invoke("session_pin", {
        id: activeId,
        specId: node.id,
        pinned: true,
      });
      applyContext(next);
      if (isRunning(detail)) {
        const refreshed = await refreshContext(bridge);
        notify(
          refreshed
            ? `${specTitle(node)} pinned and sent to the session`
            : `${specTitle(node)} pinned, but the session did not accept the refresh`,
          refreshed ? "ok" : "error",
        );
      } else {
        notify(`${specTitle(node)} pinned to this session`);
      }
    } catch (error) {
      notify(errorText(error), "error");
    }
  };

  return (
    <div className="insp-pane">
      {selected ? (
        <div className="peek-host">
          <div className="peek-head">
            <span className="peek-kind">Spec</span>
            <button
              className="icon-btn sm"
              type="button"
              aria-label="Close details"
              onClick={closeSpecDoc}
            >
              <Icon name="x" size="xs" />
            </button>
          </div>
          <h3 className="peek-name">{specTitle(selected)}</h3>
          <p className="peek-desc">{specDoc ? excerpt(specDoc.body) : "Loading…"}</p>
          <code className="peek-path">{selected.path}</code>
          <div className="peek-actions">
            <button
              className="btn-ghost sm"
              type="button"
              disabled={!activeId || pinned.has(selected.id)}
              onClick={() => void attach(selected)}
            >
              {pinned.has(selected.id) ? "Attached" : "Attach to session"}
            </button>
            <span className="prow-meta">{statusLabel(selected)}</span>
          </div>
        </div>
      ) : null}

      <div className="insp-scroll od-scroll">
        <div className="pane-head">
          <span id="spec-summary">
            {nodes.length} spec{nodes.length === 1 ? "" : "s"} · {accepted} accepted
          </span>
        </div>
        {specError ? (
          <p className="git-empty">{specError}</p>
        ) : nodes.length === 0 ? (
          <p className="git-empty">No specs indexed for this project yet.</p>
        ) : (
          nodes.map((node) => (
            <button
              className="prow"
              type="button"
              key={node.id}
              aria-expanded={specDocPath === node.path}
              onClick={() => void loadSpecDoc(bridge, node.path)}
            >
              <span className="prow-name">
                {specTitle(node)}
                {pinned.has(node.id) ? " ·" : ""}
              </span>
              <span
                className={
                  node.status?.toLowerCase() === "accepted"
                    ? "prow-meta accepted"
                    : "prow-meta"
                }
              >
                {statusLabel(node)}
              </span>
            </button>
          ))
        )}
      </div>
    </div>
  );
}
