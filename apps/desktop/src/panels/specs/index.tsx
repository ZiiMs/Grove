import type { IpcError, SpecDoc, SpecGraph, SpecNode, SpecValidation } from "@grove/contract";
import MarkdownIt from "markdown-it";
import { lazy, Suspense, useEffect, useMemo, useState } from "react";
import type { UnlistenFn } from "../../ipc/bridge";
import { useBridge } from "../../ipc/context";
import { errorText } from "../../ipc/errors";
import { subscribeToSpecChanged } from "../../ipc/events";
import { useSessionStore } from "../../state/sessionStore";
import { PanelFrame } from "../PanelFrame";
import "./specs.css";

/**
 * Specs panel: the project's spec graph on the left, one readable spec on the right.
 *
 * The index comes from the core (`spec_index`), which roots itself at the project root the app is
 * looking at and returns that root in the graph. Reading is markdown-it with HTML disabled; editing
 * mounts Monaco lazily, so the editor chunk is only downloaded when the user actually edits. Saves
 * carry the hash of the read they were based on — a mismatch is a `conflict`, surfaced with a reload
 * affordance rather than a silent overwrite.
 *
 * A spec can be pinned to the session the Sessions panel has open: a pin is durable and decides what
 * the session's harness receives as context (`session_pin`). The pin state comes from the same
 * snapshot the session header shows, so the two views can never disagree.
 */

const MonacoEditor = lazy(() => import("@monaco-editor/react"));

const markdown = new MarkdownIt({ html: false, linkify: false, typographer: false });

/**
 * How long a burst of `grove://spec-changed` events is coalesced for. An agent writing five files
 * produces five events; one index is enough, and one index is what the panel gets.
 */
export const SPEC_CHANGED_DEBOUNCE_MS = 150;

type Relation = {
  label: string;
  field: keyof Pick<SpecNode, "parent" | "dependsOn" | "references" | "implements" | "covers">;
};

const RELATIONS: Relation[] = [
  { label: "Parent", field: "parent" },
  { label: "Depends on", field: "dependsOn" },
  { label: "References", field: "references" },
  { label: "Implements", field: "implements" },
  { label: "Covers", field: "covers" },
];

function nodeLabel(node: SpecNode): string {
  return node.title ?? node.id;
}

function groupByType(nodes: SpecNode[]): Array<[string, SpecNode[]]> {
  const groups = new Map<string, SpecNode[]>();
  for (const node of nodes) {
    const group = groups.get(node.type) ?? [];
    group.push(node);
    groups.set(node.type, group);
  }
  return [...groups.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([type, group]) => [
      type,
      group.sort((left, right) => nodeLabel(left).localeCompare(nodeLabel(right))),
    ]);
}

function targetsFor(node: SpecNode, graph: SpecGraph, relation: Relation): SpecNode[] {
  const ids =
    relation.field === "parent"
      ? node.parent
        ? [node.parent]
        : []
      : (node[relation.field] as string[]);
  return ids
    .map((id) => graph.nodes.find((candidate) => candidate.id === id))
    .filter((candidate): candidate is SpecNode => candidate !== undefined);
}

function childrenOf(node: SpecNode, graph: SpecGraph): SpecNode[] {
  return graph.nodes
    .filter((candidate) => candidate.parent === node.id)
    .sort((left, right) => nodeLabel(left).localeCompare(nodeLabel(right)));
}

function isIpcError(error: unknown): error is IpcError {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    typeof (error as { code: unknown }).code === "string" &&
    "message" in error &&
    typeof (error as { message: unknown }).message === "string"
  );
}


function validationCount(validation: SpecValidation): number {
  return (
    validation.danglingLinks.length +
    validation.duplicateIds.length +
    validation.parentCycles.length
  );
}

export function SpecsPanel() {
  const bridge = useBridge();
  const activeSession = useSessionStore((state) => state.activeId);
  const sessionTitle = useSessionStore(
    (state) => state.sessions.find((row) => row.id === state.activeId)?.title ?? null,
  );
  const [pinned, setPinned] = useState<string[]>([]);
  const [pinning, setPinning] = useState(false);
  const [pinError, setPinError] = useState<string | null>(null);
  const [graph, setGraph] = useState<SpecGraph | null>(null);
  const [graphError, setGraphError] = useState<string | null>(null);
  const [indexing, setIndexing] = useState(true);
  const [indexToken, setIndexToken] = useState(0);

  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [doc, setDoc] = useState<SpecDoc | null>(null);
  const [docError, setDocError] = useState<string | null>(null);
  const [loadingDoc, setLoadingDoc] = useState(false);
  const [reloadToken, setReloadToken] = useState(0);

  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [saving, setSaving] = useState(false);
  const [conflict, setConflict] = useState<IpcError | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);

  useEffect(() => {
    if (!activeSession) {
      setPinned([]);
      return;
    }
    let cancelled = false;
    bridge
      .invoke("session_context_get", { id: activeSession })
      .then((context) => {
        if (!cancelled) setPinned(context.pins.map((pin) => pin.specId));
      })
      .catch(() => {
        // A session that cannot be read pins nothing; the toggle stays disabled.
        if (!cancelled) setPinned([]);
      });
    return () => {
      cancelled = true;
    };
  }, [bridge, activeSession]);

  useEffect(() => {
    let cancelled = false;
    setIndexing(true);
    setGraphError(null);
    bridge
      .invoke("spec_index")
      .then((next) => {
        if (!cancelled) setGraph(next);
      })
      .catch((error: unknown) => {
        if (!cancelled) setGraphError(errorText(error));
      })
      .finally(() => {
        if (!cancelled) setIndexing(false);
      });
    return () => {
      cancelled = true;
    };
  }, [bridge, indexToken]);

  // The core announces spec changes (`grove://spec-changed`) from its watcher, and also when the
  // project root moves with the active session. Either way the graph on screen is now stale: re-index,
  // debounced so a burst of writes costs one pass.
  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    void subscribeToSpecChanged(bridge, () => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => setIndexToken((token) => token + 1), SPEC_CHANGED_DEBOUNCE_MS);
    }).then((stop) => {
      if (cancelled) stop();
      else unlisten = stop;
    });
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
      unlisten?.();
    };
  }, [bridge]);

  useEffect(() => {
    if (!selectedPath) {
      setDoc(null);
      return;
    }
    let cancelled = false;
    setLoadingDoc(true);
    setDocError(null);
    setConflict(null);
    setSaveError(null);
    setEditing(false);
    setDraft("");
    bridge
      .invoke("spec_read", { path: selectedPath })
      .then((next) => {
        if (!cancelled) setDoc(next);
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setDoc(null);
          setDocError(errorText(error));
        }
      })
      .finally(() => {
        if (!cancelled) setLoadingDoc(false);
      });
    return () => {
      cancelled = true;
    };
  }, [bridge, selectedPath, reloadToken]);

  const groups = useMemo(() => (graph ? groupByType(graph.nodes) : []), [graph]);
  const rendered = useMemo(() => (doc ? markdown.render(doc.body) : ""), [doc]);

  function open(path: string) {
    setSelectedPath(path);
  }

  function beginEdit() {
    if (!doc) return;
    setDraft(doc.body);
    setEditing(true);
    setConflict(null);
    setSaveError(null);
  }

  async function save() {
    if (!doc || saving) return;
    setSaving(true);
    setConflict(null);
    setSaveError(null);
    try {
      const next = await bridge.invoke("spec_write", {
        path: doc.node.path,
        body: draft,
        expectedHash: doc.hash,
      });
      setDoc(next);
      setEditing(false);
      setDraft("");
    } catch (error: unknown) {
      if (isIpcError(error) && error.code === "conflict") {
        setConflict(error);
      } else {
        setSaveError(errorText(error));
      }
    } finally {
      setSaving(false);
    }
  }

  async function togglePin(specId: string) {
    if (!activeSession || pinning) return;
    const next = !pinned.includes(specId);
    setPinning(true);
    setPinError(null);
    try {
      const context = await bridge.invoke("session_pin", {
        id: activeSession,
        specId,
        pinned: next,
      });
      setPinned(context.pins.map((pin) => pin.specId));
      // The session header shows the same snapshot: adopt it so pinning here is visible there.
      useSessionStore.getState().applyContext(context);
    } catch (error: unknown) {
      setPinError(errorText(error));
    } finally {
      setPinning(false);
    }
  }

  function reload() {
    setConflict(null);
    setReloadToken((token) => token + 1);
  }

  return (
    <PanelFrame
      title="Specs"
      subtitle={graph ? graph.root : indexing ? "Indexing project specs…" : "Spec graph"}
      actions={
        <button type="button" className="input" onClick={() => setIndexToken((token) => token + 1)}>
          Reindex
        </button>
      }
    >
      <div className="specs-layout" data-testid="specs-panel">
        <nav className="spec-tree" aria-label="Spec tree">
          {indexing && !graph ? <p className="specs-note">Indexing…</p> : null}
          {graphError ? (
            <p className="specs-error" role="alert">
              {graphError}
            </p>
          ) : null}
          {graph && graph.nodes.length === 0 && !graphError ? (
            <p className="specs-note">No spec nodes found under {graph.root}.</p>
          ) : null}
          {groups.map(([type, nodes]) => (
            <section key={type} className="spec-group">
              <h2 className="spec-group-title">{type}</h2>
              <ul className="spec-group-list">
                {nodes.map((node) => (
                  <li key={`${node.id}:${node.path}`}>
                    <button
                      type="button"
                      className={
                        node.path === selectedPath ? "spec-tree-item is-active" : "spec-tree-item"
                      }
                      aria-current={node.path === selectedPath ? "true" : undefined}
                      onClick={() => open(node.path)}
                    >
                      <span className="spec-tree-name">
                        {nodeLabel(node)}
                        {pinned.includes(node.id) ? (
                          <span className="spec-pin-badge" title="Pinned to the open session">
                            pinned
                          </span>
                        ) : null}
                      </span>
                      <span className="spec-tree-meta">
                        <code>{node.id}</code>
                        {node.status ? <span className="spec-status">{node.status}</span> : null}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          ))}
          {graph && validationCount(graph.validation) > 0 ? (
            <section className="spec-validation" role="status" aria-label="Spec validation">
              <h2 className="spec-group-title">Validation</h2>
              <ul className="spec-group-list">
                {graph.validation.danglingLinks.map((link) => (
                  <li key={`dangling:${link.path}:${link.field}:${link.target}`}>
                    <code>{link.path}</code> {link.field} → missing <code>{link.target}</code>
                  </li>
                ))}
                {graph.validation.duplicateIds.map((duplicate) => (
                  <li key={`duplicate:${duplicate.id}`}>
                    duplicate id <code>{duplicate.id}</code> in {duplicate.paths.join(", ")}
                  </li>
                ))}
                {graph.validation.parentCycles.map((cycle) => (
                  <li key={`cycle:${cycle.ids.join(":")}`}>
                    parent cycle {cycle.ids.join(" → ")} ({cycle.paths.join(", ")})
                  </li>
                ))}
              </ul>
            </section>
          ) : null}
        </nav>

        <article className="spec-doc" aria-label="Spec document">
          {!selectedPath ? (
            <p className="specs-note">Select a spec to read it.</p>
          ) : loadingDoc ? (
            <p className="specs-note">Loading…</p>
          ) : docError ? (
            <p className="specs-error" role="alert">
              {docError}
            </p>
          ) : doc ? (
            <>
              <header className="spec-doc-header">
                <div>
                  <h2 className="spec-doc-title">{nodeLabel(doc.node)}</h2>
                  <p className="spec-doc-meta">
                    <span>{doc.node.type}</span>
                    <span>·</span>
                    <code>{doc.node.path}</code>
                    {doc.node.status ? <span className="spec-status">{doc.node.status}</span> : null}
                  </p>
                </div>
                <div className="spec-doc-actions">
                  {editing ? (
                    <>
                      <button type="button" className="input primary" onClick={() => void save()} disabled={saving}>
                        {saving ? "Saving…" : "Save"}
                      </button>
                      <button type="button" className="input" onClick={() => setEditing(false)} disabled={saving}>
                        Cancel
                      </button>
                    </>
                  ) : (
                    <>
                      <button
                        type="button"
                        className="input"
                        data-testid="spec-pin"
                        aria-pressed={pinned.includes(doc.node.id)}
                        disabled={!activeSession || pinning}
                        title={
                          activeSession
                            ? `${pinned.includes(doc.node.id) ? "Unpin from" : "Pin to"} ${sessionTitle ?? "the open session"}`
                            : "Open a session to pin specs to it"
                        }
                        onClick={() => void togglePin(doc.node.id)}
                      >
                        {pinned.includes(doc.node.id) ? "Unpin from session" : "Pin to session"}
                      </button>
                      <button type="button" className="input" onClick={beginEdit}>
                        Edit
                      </button>
                    </>
                  )}
                </div>
              </header>

              <p className="spec-pin-note" data-testid="spec-pin-note">
                {activeSession
                  ? `Context for ${sessionTitle ?? activeSession}: a pinned spec is sent to this session's harness on start and on Refresh context.`
                  : "No session is open. Open one in Sessions to pin this spec to it."}
              </p>
              {pinError ? (
                <p className="specs-error" role="alert" data-testid="spec-pin-error">
                  {pinError}
                </p>
              ) : null}

              {conflict ? (
                <div className="spec-conflict" role="alert">
                  <p>
                    This spec changed on disk since you opened it. Your edit was not saved — reload
                    the newer content and try again.
                  </p>
                  <button type="button" className="input primary" onClick={reload}>
                    Reload
                  </button>
                </div>
              ) : null}
              {saveError ? (
                <p className="specs-error" role="alert">
                  {saveError}
                </p>
              ) : null}

              {!editing ? (
                <>
                  <SpecLinks node={doc.node} graph={graph} onOpen={open} />
                  <div
                    className="spec-markdown"
                    data-testid="spec-reader"
                    dangerouslySetInnerHTML={{ __html: rendered }}
                  />
                </>
              ) : (
                <div className="spec-editor" data-testid="spec-editor">
                  <Suspense fallback={<p className="specs-note">Loading editor…</p>}>
                    <MonacoEditor
                      height="420px"
                      defaultLanguage="markdown"
                      value={draft}
                      onChange={(value) => setDraft(value ?? "")}
                      theme="vs-dark"
                      options={{
                        minimap: { enabled: false },
                        wordWrap: "on",
                        scrollBeyondLastLine: false,
                      }}
                    />
                  </Suspense>
                </div>
              )}
            </>
          ) : (
            <p className="specs-note">Select a spec to read it.</p>
          )}
        </article>
      </div>
    </PanelFrame>
  );
}

function SpecLinks({
  node,
  graph,
  onOpen,
}: {
  node: SpecNode;
  graph: SpecGraph | null;
  onOpen: (path: string) => void;
}) {
  if (!graph) return null;
  const children = childrenOf(node, graph);
  const relations = RELATIONS.map((relation) => ({
    relation,
    nodes: targetsFor(node, graph, relation),
  })).filter((entry) => entry.nodes.length > 0);

  if (relations.length === 0 && children.length === 0) return null;

  return (
    <div className="spec-links" aria-label="Spec links">
      {relations.map(({ relation, nodes }) => (
        <div key={relation.field} className="spec-link-group">
          <span className="spec-link-label">{relation.label}</span>
          {nodes.map((target) => (
            <button
              key={target.id}
              type="button"
              className="spec-link"
              aria-label={`Open ${nodeLabel(target)}`}
              onClick={() => onOpen(target.path)}
            >
              {nodeLabel(target)}
            </button>
          ))}
        </div>
      ))}
      {children.length > 0 ? (
        <div className="spec-link-group">
          <span className="spec-link-label">Children</span>
          {children.map((target) => (
            <button
              key={target.id}
              type="button"
              className="spec-link"
              aria-label={`Open ${nodeLabel(target)}`}
              onClick={() => onOpen(target.path)}
            >
              {nodeLabel(target)}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}
