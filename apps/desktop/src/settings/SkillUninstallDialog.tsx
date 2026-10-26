import { Dialog } from "../dialogs/Dialog";

/** Confirmation for deleting every skill copy Grove can see. */
export function SkillUninstallDialog({
  open,
  count,
  busy,
  onClose,
  onConfirm,
}: {
  open: boolean;
  count: number;
  busy: boolean;
  onClose: () => void;
  onConfirm: () => void;
}) {
  return (
    <Dialog open={open} className="skill-uninstall-dialog" labelledBy="su-title" onClose={onClose}>
      <div className="dlg-head">
        <h2 id="su-title">Uninstall all skills?</h2>
      </div>
      <p className="dlg-sub">
        This deletes {count} installed skill cop{count === 1 ? "y" : "ies"} from the{" "}
        <span className="mono">.agents/skills</span> folders Grove can see. This cannot be undone.
      </p>
      <div className="dlg-foot">
        <button className="btn-ghost" type="button" onClick={onClose}>
          Cancel
        </button>
        <button className="btn-ghost danger" type="button" disabled={busy} onClick={onConfirm}>
          {busy ? "Removing…" : "Uninstall all"}
        </button>
      </div>
    </Dialog>
  );
}
