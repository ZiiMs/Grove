import type { Plan, SessionSummary } from "@grove/contract";
import MarkdownIt from "markdown-it";
import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { useBridge } from "../../ipc/context";
import { errorText } from "../../ipc/errors";
import { subscribeToPlanChanged } from "../../ipc/events";
import { useSessionStore } from "../../state/sessionStore";
import { PanelFrame } from "../PanelFrame";
import "./plan.css";

/**
 * Plan panel: one versioned TODO plan per session, edited here and by the agent as a file.
 *
 * The core owns the dual-edit rules (`PlanService`): a save carries the version it was based on, an
 * external edit is imported on read, and a stale save comes back as `conflict`. The panel's job is to
 * show that honestly — save with the loaded version, and on a conflict keep the draft and offer a
 * reload instead of overwriting the other writer. The mirror path is shown with a copy affordance,
 * because that path is the agent's half of the deal.
 *
 * The mirror is a plain file, so the panel re-reads it on window focus and on a slow timer while it
 * is open; `plan_get` imports an external edit and the core announces it on `grove://plan-changed`.
 * Markdown is rendered with HTML disabled, like every other markdown surface in the app.
 */

const markdown = new MarkdownIt({ html: false, linkify: false, typographer: false });

/** How often an open panel re-checks the mirror for an agent edit. */
export const PLAN_POLL_MS = 3000;

function isConflict(error: unknown): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    (error as { code?: unknown }).code === "conflict"
  );
}


export function PlanPanel() {
  const bridge = useBridge();
  const activeId = useSessionStore((state) => state.activeId);

  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [plan, setPlan] = useState<Plan | null>(null);
  const [draft, setDraft] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conflict, setConflict] = useState<string | null>(null);
  const [copyNote, setCopyNote] = useState<string | null>(null);
  const [reloadToken, setReloadToken] = useState(0);

  // The external-update decision reads the current plan and draft from refs, so the polling timer
  // never captures a stale render.
  const planRef = useRef<Plan | null>(null);
  const draftRef = useRef("");
  useEffect(() => {
    planRef.current = plan;
  }, [plan]);
  useEffect(() => {
    draftRef.current = draft;
  }, [draft]);

  // The list: the panel mounts alone, so it loads the sessions itself. It prefers the session the
  // Sessions panel has open, then the first session — never a second notion of "current".
  useEffect(() => {
    let cancelled = false;
    bridge
      .invoke("session_list")
      .then((rows) => {
        if (cancelled) return;
        setSessions(rows);
        setSessionId((current) => {
          if (current && rows.some((row) => row.id === current)) return current;
          if (activeId && rows.some((row) => row.id === activeId)) return activeId;
          return rows[0]?.id ?? null;
        });
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(errorText(err));
      });
    return () => {
      cancelled = true;
    };
  }, [bridge, activeId]);

  useEffect(() => {
    if (!sessionId) {
      setPlan(null);
      setDraft("");
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError(null);
    bridge
      .invoke("plan_get", { id: sessionId })
      .then((next) => {
        if (cancelled) return;
        setPlan(next);
        setDraft(next.markdown);
        setConflict(null);
        setCopyNote(null);
        setLoading(false);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setError(errorText(err));
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [bridge, sessionId, reloadToken]);

  /**
   * Fold a plan the core produced from an external read into the editor. A clean draft is replaced;
   * a dirty one is kept and the user is offered a reload, so an agent edit can never silently
   * discard the human's typing.
   */
  const adopt = useCallback((next: Plan) => {
    const current = planRef.current;
    if (!current) return;
    if (next.version === current.version) return;
    if (draftRef.current !== current.markdown) {
      setConflict(
        `The agent changed the plan on disk (version ${next.version}). Reload to see it before saving.`,
      );
      return;
    }
    setPlan(next);
    setDraft(next.markdown);
    setConflict(null);
  }, []);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void subscribeToPlanChanged(bridge, (changedSessionId, next) => {
      if (changedSessionId && changedSessionId !== sessionId) return;
      adopt(next);
    }).then((stop) => {
      if (active) unlisten = stop;
      else stop();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }, [bridge, sessionId, adopt]);

  // The agent writes a file: re-read it while the panel is open and when the window regains focus.
  // `plan_get` does the import; an actual change also arrives on `grove://plan-changed`.
  useEffect(() => {
    if (!sessionId) return;
    const refresh = () => {
      bridge
        .invoke("plan_get", { id: sessionId })
        .then(adopt)
        .catch(() => {
          // A transient read failure is not worth a banner; a save or the next check reports it.
        });
    };
    const timer = window.setInterval(refresh, PLAN_POLL_MS);
    window.addEventListener("focus", refresh);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", refresh);
    };
  }, [bridge, sessionId, adopt]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!sessionId || !plan || saving) return;
    setSaving(true);
    setError(null);
    try {
      const next = await bridge.invoke("plan_set", {
        id: sessionId,
        update: { markdown: draft, expectedVersion: plan.version },
      });
      setPlan(next);
      setDraft(next.markdown);
      setConflict(null);
      setCopyNote(null);
    } catch (err: unknown) {
      if (isConflict(err)) setConflict(errorText(err));
      else setError(errorText(err));
    } finally {
      setSaving(false);
    }
  };

  const copyMirrorPath = async () => {
    if (!plan) return;
    try {
      const clipboard = typeof navigator === "undefined" ? undefined : navigator.clipboard;
      if (!clipboard) throw new Error("clipboard unavailable");
      await clipboard.writeText(plan.mirrorPath);
      setCopyNote("Copied");
    } catch {
      setCopyNote("Copy failed — select the path and copy it manually.");
    }
  };

  const rendered = useMemo(() => markdown.render(draft), [draft]);
  const dirty = plan !== null && draft !== plan.markdown;

  return (
    <PanelFrame
      title="Plan"
      subtitle="One versioned TODO plan per session. You edit it here; the agent edits the mirror file."
    >
      <form className="plan" data-testid="plan-body" onSubmit={submit}>
        <div className="plan-toolbar">
          <label className="field">
            <span>Session</span>
            <select
              className="input"
              aria-label="Plan session"
              data-testid="plan-session"
              value={sessionId ?? ""}
              onChange={(event) => setSessionId(event.target.value || null)}
            >
              {sessions.length === 0 ? <option value="">No sessions yet</option> : null}
              {sessions.map((row) => (
                <option key={row.id} value={row.id}>
                  {row.title}
                </option>
              ))}
            </select>
          </label>
          <span className="plan-version" data-testid="plan-version">
            version {plan?.version ?? 0}
          </span>
          {dirty ? (
            <span className="plan-dirty" data-testid="plan-dirty">
              unsaved changes
            </span>
          ) : null}
          <button
            type="button"
            className="input"
            data-testid="plan-reload"
            onClick={() => setReloadToken((token) => token + 1)}
          >
            Reload
          </button>
          <button
            type="submit"
            className="input primary"
            data-testid="plan-save"
            disabled={!dirty || saving || !plan}
          >
            {saving ? "Saving…" : "Save"}
          </button>
        </div>

        {plan ? (
          <div className="plan-mirror" data-testid="plan-mirror">
            <span className="plan-mirror-label">Agent mirror</span>
            <code className="plan-mirror-path" data-testid="plan-mirror-path">
              {plan.mirrorPath}
            </code>
            <button
              type="button"
              className="input"
              data-testid="plan-copy-path"
              onClick={() => void copyMirrorPath()}
            >
              Copy path
            </button>
            {copyNote ? (
              <span className="plan-copy-note" data-testid="plan-copy-note">
                {copyNote}
              </span>
            ) : null}
          </div>
        ) : null}

        {conflict ? (
          <div className="plan-conflict" role="alert" data-testid="plan-conflict">
            <p>{conflict}</p>
            <button
              type="button"
              className="input"
              data-testid="plan-conflict-reload"
              onClick={() => setReloadToken((token) => token + 1)}
            >
              Reload the plan
            </button>
          </div>
        ) : null}

        {error ? (
          <p className="plan-error" role="alert" data-testid="plan-error">
            {error}
          </p>
        ) : null}

        {loading ? <p className="plan-empty">Loading plan…</p> : null}
        {!loading && !sessionId ? (
          <p className="plan-empty" data-testid="plan-empty">
            Create a session to give it a plan.
          </p>
        ) : null}

        {plan ? (
          <div className="plan-grid">
            <label className="plan-pane">
              <span className="plan-pane-title">Markdown</span>
              <textarea
                className="input plan-editor"
                aria-label="Plan editor"
                data-testid="plan-editor"
                rows={18}
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
              />
            </label>
            <section className="plan-pane">
              <span className="plan-pane-title">Preview</span>
              <div
                className="plan-preview"
                data-testid="plan-preview"
                dangerouslySetInnerHTML={{ __html: rendered }}
              />
            </section>
          </div>
        ) : null}
      </form>
    </PanelFrame>
  );
}
