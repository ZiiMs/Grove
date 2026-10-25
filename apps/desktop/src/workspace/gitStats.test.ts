import { describe, expect, it } from "vitest";
import { statsFromPatch } from "./gitStats";

const PATCH = [
  "diff --git a/session.rs b/session.rs",
  "index 111..222 100644",
  "--- a/session.rs",
  "+++ b/session.rs",
  "@@ -1,2 +1,3 @@",
  " context",
  "-old line",
  "+new line",
  "+added line",
  "diff --git a/new.ts b/new.ts",
  "new file mode 100644",
  "--- /dev/null",
  "+++ b/new.ts",
  "@@ -0,0 +1,1 @@",
  "+export {};",
  "diff --git a/gone.md b/gone.md",
  "deleted file mode 100644",
  "--- a/gone.md",
  "+++ /dev/null",
  "@@ -1,1 +0,0 @@",
  "-bye",
].join("\n");

describe("statsFromPatch", () => {
  it("counts added and removed lines per file", () => {
    expect(statsFromPatch(PATCH)).toEqual({
      "session.rs": { added: 2, removed: 1 },
      "new.ts": { added: 1, removed: 0 },
      "gone.md": { added: 0, removed: 1 },
    });
  });

  it("ignores headers and text before the first file", () => {
    expect(statsFromPatch("noise\n+not a diff")).toEqual({});
    expect(statsFromPatch("")).toEqual({});
  });
});
