import { useEffect, useState } from "react";
import { Dialog } from "../dialogs/Dialog";
import { Icon } from "../workspace/Icon";

/** Add one GitHub repository to the skill catalog sources. */
export function SkillSourceDialog({
  open,
  busy,
  error,
  onClose,
  onDirty,
  onAdd,
}: {
  open: boolean;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onDirty: () => void;
  onAdd: (input: string) => void;
}) {
  const [value, setValue] = useState("");

  useEffect(() => {
    if (open) setValue("");
  }, [open]);

  const submit = () => {
    const input = value.trim();
    if (!input || busy) return;
    onAdd(input);
  };

  return (
    <Dialog open={open} className="skill-source-dialog" labelledBy="sc-title" onClose={onClose}>
      <div className="dlg-head">
        <h2 id="sc-title">Add catalog</h2>
        <button className="icon-btn sm" type="button" aria-label="Close dialog" onClick={onClose}>
          <Icon name="x" size="sm" />
        </button>
      </div>

      <div className="field">
        <label htmlFor="sc-repo">GitHub repository</label>
        <input
          id="sc-repo"
          className="mono-input"
          type="text"
          placeholder="owner/repo"
          autoComplete="off"
          spellCheck={false}
          data-autofocus
          value={value}
          onChange={(event) => {
            setValue(event.target.value);
            onDirty();
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              submit();
            }
          }}
        />
        <p className="field-hint">Paste an owner/repo or a GitHub URL.</p>
      </div>

      {error ? (
        <p className="field-err" role="alert">
          {error}
        </p>
      ) : null}

      <div className="dlg-foot">
        <button className="btn-ghost" type="button" onClick={onClose}>
          Cancel
        </button>
        <button
          className="btn-primary"
          type="button"
          disabled={busy || !value.trim()}
          onClick={submit}
        >
          {busy ? "Adding…" : "Add catalog"}
        </button>
      </div>
    </Dialog>
  );
}
