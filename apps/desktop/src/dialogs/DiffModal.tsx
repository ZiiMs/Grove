import type { GitDiff } from "@grove/contract";
import { useEffect, useState } from "react";
import { useBridge } from "../ipc/context";
import { useContextStore } from "../state/contextStore";
import { useUiStore } from "../state/uiStore";
import { Icon } from "../workspace/Icon";
import { basename, dirname } from "../workspace/tree";
import { Dialog } from "./Dialog";

interface Row {
  kind: "ctx" | "add" | "del" | "hunk";
  oldNo: string;
  newNo: string;
  text: string;
}

function parsePatch(patch: string): Row[] {
  const rows: Row[] = [];
  let oldNo = 0;
  let newNo = 0;
  for (const line of patch.split("\n")) {
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)$/.exec(line);
    if (hunk) {
      oldNo = Number(hunk[1]);
      newNo = Number(hunk[2]);
      rows.push({ kind: "hunk", oldNo: "", newNo: "", text: line });
      continue;
    }
    if (line.startsWith("\\")) continue;
    // File headers are metadata, not content: the file list and patch head already name the file.
    if (line.startsWith("--- ") || line.startsWith("+++ ")) continue;
    if (line.startsWith("+")) {
      rows.push({ kind: "add", oldNo: "", newNo: String(newNo++), text: line.slice(1) });
    } else if (line.startsWith("-")) {
      rows.push({ kind: "del", oldNo: String(oldNo++), newNo: "", text: line.slice(1) });
    } else if (line.startsWith(" ")) {
      rows.push({
        kind: "ctx",
        oldNo: String(oldNo++),
        newNo: String(newNo++),
        text: line.slice(1),
      });
    }
  }
  return rows;
}

/** Read-only patch view. Staging happens in the Git tab; this is the inspector. */
export function DiffModal() {
  const bridge = useBridge();
  const path = useUiStore((state) => state.diffPath);
  const close = useUiStore((state) => state.closeDiff);
  const openDiff = useUiStore((state) => state.openDiff);
  const git = useContextStore((state) => state.git);
  const fileStats = useContextStore((state) => state.fileStats);
  const diff = useContextStore((state) => state.diff);
  const [patch, setPatch] = useState<GitDiff | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!path) {
      setPatch(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    void diff(bridge, path).then((next) => {
      if (!cancelled) {
        setPatch(next);
        setLoading(false);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [bridge, diff, path]);

  if (!path) return null;
  const rows = patch ? parsePatch(patch.patch) : [];
  const stat = fileStats[path] ?? { added: 0, removed: 0 };
  const files = git?.files ?? [];

  return (
    <Dialog open={Boolean(path)} className="diff-modal" labelledBy="dd-title" onClose={close}>
      <div className="dd-shell">
        <header className="dd-head">
          <span className="dd-kicker">Diff</span>
          <h2 id="dd-title">{basename(path)}</h2>
          <span className="dd-stat">
            <span className="add">+{stat.added}</span>
            <span className="del">−{stat.removed}</span>
          </span>
          <button className="icon-btn sm" type="button" aria-label="Close diff" onClick={close}>
            <Icon name="x" size="sm" />
          </button>
        </header>
        <div className="dd-body">
          <div className="dd-files od-scroll" aria-label="Changed files">
            {files.map((file) => {
              const fileStat = fileStats[file.path] ?? { added: 0, removed: 0 };
              return (
                <button
                  className="dd-file"
                  type="button"
                  key={file.path}
                  aria-current={file.path === path}
                  onClick={() => openDiff(file.path)}
                >
                  <span style={{ minWidth: 0, flex: 1 }}>
                    <span className="fn">{basename(file.path)}</span>
                    <span className="fd">{dirname(file.path) || "workspace root"}</span>
                  </span>
                  <span className="fs">
                    <span className="add">+{fileStat.added}</span>
                    <span className="del">−{fileStat.removed}</span>
                  </span>
                </button>
              );
            })}
          </div>
          <div className="dd-patch od-scroll" aria-label="Patch">
            {loading ? (
              <p className="git-empty" style={{ padding: "14px 10px" }}>
                Reading the diff…
              </p>
            ) : rows.length === 0 ? (
              <p className="git-empty" style={{ padding: "14px 10px" }}>
                No changes to show.
              </p>
            ) : (
              <div className="patch">
                <div className="patch-head">
                  <span className="p-title">{path}</span>
                  <span className="dfile-stat">
                    <span className="add">+{stat.added}</span>
                    <span className="del">−{stat.removed}</span>
                  </span>
                </div>
                <div className="patch-note">Read-only · staging happens in the Git tab</div>
                {rows.map((row, index) =>
                  row.kind === "hunk" ? (
                    <div className="hunk-h" key={index}>
                      {row.text}
                    </div>
                  ) : (
                    <div className={`dl ${row.kind === "ctx" ? "" : row.kind}`} key={index}>
                      <span className="dl-sign">
                        {row.kind === "add" ? "+" : row.kind === "del" ? "−" : " "}
                      </span>
                      <span className="dl-no">{row.oldNo}</span>
                      <span className="dl-no">{row.newNo}</span>
                      <span className="dl-code">{row.text || " "}</span>
                    </div>
                  ),
                )}
              </div>
            )}
          </div>
        </div>
        <footer className="dd-foot">
          <span>Read-only. Staging happens in the Git panel.</span>
        </footer>
      </div>
    </Dialog>
  );
}
