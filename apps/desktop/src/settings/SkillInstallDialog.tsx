import type { CatalogEntry, SkillTarget } from "@grove/contract";
import { useEffect, useState } from "react";
import { Dialog } from "../dialogs/Dialog";
import { Combo } from "../shell/Combo";
import { Icon } from "../workspace/Icon";

/** A compact location prompt: choose the scope, then install. */
export function SkillInstallDialog({
  entries,
  targets,
  preferredTarget,
  busy,
  onClose,
  onInstall,
}: {
  /** The skills queued for install; null keeps the dialog closed. */
  entries: CatalogEntry[] | null;
  targets: SkillTarget[];
  /** `user` for a global install, `agents` when the session belongs to a project. */
  preferredTarget: string;
  busy: boolean;
  onClose: () => void;
  onInstall: (targetId: string) => void;
}) {
  const open = entries !== null && entries.length > 0;
  const defaultTargetId = targets.some((target) => target.id === preferredTarget)
    ? preferredTarget
    : (targets[0]?.id ?? preferredTarget);
  const [targetId, setTargetId] = useState(defaultTargetId);

  useEffect(() => {
    if (!open) return;
    setTargetId(defaultTargetId);
  }, [defaultTargetId, open]);

  const title =
    entries && entries.length === 1
      ? `Install ${entries[0].name}`
      : `Install ${entries?.length ?? 0} skills`;

  return (
    <Dialog open={open} className="skill-install-dialog" labelledBy="si-title" onClose={onClose}>
      <div className="dlg-head">
        <h2 id="si-title">{title}</h2>
        <button className="icon-btn sm" type="button" aria-label="Close dialog" onClick={onClose}>
          <Icon name="x" size="sm" />
        </button>
      </div>

      <div className="field">
        <label htmlFor="si-target">Install to</label>
        <Combo
          id="si-target"
          label="Install to"
          value={targetId}
          groups={[
            {
              options: targets.map((target) => ({
                value: target.id,
                label: target.label,
              })),
            },
          ]}
          disabled={busy}
          onChange={setTargetId}
        />
      </div>

      <div className="dlg-foot">
        <button className="btn-ghost" type="button" onClick={onClose}>
          Cancel
        </button>
        <button
          className="btn-primary"
          type="button"
          disabled={busy || targets.length === 0}
          onClick={() => onInstall(targetId)}
        >
          {busy ? "Installing…" : "Install"}
        </button>
      </div>
    </Dialog>
  );
}
