import type { GitFileStatus } from "@grove/contract";
import { describe, expect, it } from "vitest";
import { statsFromFiles } from "./gitStats";

const file = (path: string, added: number, removed: number): GitFileStatus => ({
  path,
  from: null,
  index: "M",
  worktree: " ",
  staged: false,
  untracked: false,
  added,
  removed,
});

describe("statsFromFiles", () => {
  it("maps each file's counts and sums the checkout", () => {
    const { fileStats, total } = statsFromFiles([
      file("session.rs", 2, 1),
      file("new.ts", 1, 0),
      file("gone.md", 0, 1),
    ]);
    expect(fileStats).toEqual({
      "session.rs": { added: 2, removed: 1 },
      "new.ts": { added: 1, removed: 0 },
      "gone.md": { added: 0, removed: 1 },
    });
    expect(total).toEqual({ added: 3, removed: 2 });
  });

  it("is empty for a clean checkout", () => {
    expect(statsFromFiles([])).toEqual({ fileStats: {}, total: { added: 0, removed: 0 } });
  });
});
