import type { Project } from "@grove/contract";
import { useEffect, useState } from "react";
import { useBridge } from "../ipc/context";
import { errorText } from "../ipc/errors";
import { useContextStore } from "../state/contextStore";
import { useUiStore } from "../state/uiStore";
import { Icon } from "../workspace/Icon";
import { Dialog } from "./Dialog";

/** The pull-request sheet: title, body, base/head preview and the draft switch. */
export function PrDialog({ project }: { project: Project | null }) {
  const bridge = useBridge();
  const open = useUiStore((state) => state.prOpen);
  const seed = useUiStore((state) => state.prSeed);
  const close = useUiStore((state) => state.closePr);
  const notify = useUiStore((state) => state.notify);
  const git = useContextStore((state) => state.git);
  const openPr = useContextStore((state) => state.openPr);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [draft, setDraft] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [working, setWorking] = useState(false);

  useEffect(() => {
    if (!open) return;
    setTitle(seed?.title ?? "");
    setBody(seed?.body ?? "");
    setDraft(false);
    setError(null);
    setWorking(false);
  }, [open, seed]);

  const base = project?.defaultBranch ?? "main";
  const head = git?.branch ?? "";

  const create = async () => {
    if (!title.trim()) {
      setError("A pull request needs a title.");
      return;
    }
    setWorking(true);
    setError(null);
    try {
      const result = await openPr(bridge, {
        title: title.trim(),
        body,
        base,
        head: head || undefined,
        draft,
      });
      if (result) {
        close();
        notify(`Pull request opened${draft ? " as draft" : ""}: ${result.url}`);
      }
    } catch (err) {
      setError(errorText(err));
    } finally {
      setWorking(false);
    }
  };

  return (
    <Dialog open={open} labelledBy="pr-title" onClose={close}>
      <div className="dlg-head">
        <h2 id="pr-title">Open pull request</h2>
        <button className="icon-btn sm" type="button" aria-label="Close pull request dialog" onClick={close}>
          <Icon name="x" size="sm" />
        </button>
      </div>
      <p className="dlg-sub">
        A commit is created first, then the branch is pushed and the pull request opened.
      </p>
      <div className="field">
        <label htmlFor="pr-title-input">Title</label>
        <input
          id="pr-title-input"
          type="text"
          autoComplete="off"
          value={title}
          onChange={(event) => {
            setTitle(event.target.value);
            setError(null);
          }}
        />
      </div>
      <div className="field">
        <label htmlFor="pr-body">Description</label>
        <textarea
          id="pr-body"
          rows={5}
          value={body}
          onChange={(event) => setBody(event.target.value)}
        />
      </div>
      <div className="grid-2">
        <div className="field-tight">
          <span className="field-label">Base branch</span>
          <p className="path-preview">{base}</p>
        </div>
        <div className="field-tight">
          <span className="field-label">Head</span>
          <p className="path-preview">{head || "current branch"}</p>
        </div>
      </div>
      <label className="switch-row" htmlFor="pr-draft" style={{ marginTop: 13 }}>
        <input
          className="switch-input"
          type="checkbox"
          id="pr-draft"
          checked={draft}
          onChange={(event) => setDraft(event.target.checked)}
        />
        <span className="switch" aria-hidden="true" />
        <span className="switch-copy">
          <span className="switch-title">Open as draft</span>
          <span className="switch-sub">
            Reviewers are not requested until you mark it ready.
          </span>
        </span>
      </label>
      {error ? (
        <p className="field-err" role="alert">
          {error}
        </p>
      ) : null}
      <div className="dlg-foot">
        <button className="btn-ghost" type="button" onClick={close}>
          Cancel
        </button>
        <button className="btn-primary" type="button" disabled={working} onClick={() => void create()}>
          {working ? "Opening…" : "Create pull request"}
        </button>
      </div>
    </Dialog>
  );
}
