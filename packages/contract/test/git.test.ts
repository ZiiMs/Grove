import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  gitBranchSchema,
  gitDiffSchema,
  gitStatusSchema,
  gitWorktreeSchema,
  parseGitChanged,
  prResultSchema,
} from "../src/index";

function fixture(name: string): unknown {
  const path = fileURLToPath(new URL(`../../../contract/fixtures/${name}`, import.meta.url));
  return JSON.parse(readFileSync(path, "utf8"));
}

describe("git contract fixtures", () => {
  it("git-status.json matches the zod schema", () => {
    const parsed = gitStatusSchema.parse(fixture("git-status.json"));
    expect(parsed.branch).toBe("ticket/04-git-panel");
    expect(parsed.files).toHaveLength(4);
    expect(parsed.files.find((file) => file.untracked)?.path).toBe("scratch.txt");
  });

  it("git-branch.json matches the zod schema", () => {
    const parsed = gitBranchSchema.parse(fixture("git-branch.json"));
    expect(parsed.ahead).toBe(2);
    expect(parsed.behind).toBe(1);
    expect(parsed.remote).toBeNull();
  });

  it("git-worktree.json matches the zod schema", () => {
    const parsed = gitWorktreeSchema.parse(fixture("git-worktree.json"));
    expect(parsed.current).toBe(true);
    expect(parsed.branch).toBe("ticket/04-git-panel");
  });

  it("git-diff.json matches the zod schema", () => {
    const parsed = gitDiffSchema.parse(fixture("git-diff.json"));
    expect(parsed.target).toBe("working_tree");
    expect(parsed.patch).toContain("+const answer = 42;");
    expect(parsed.truncated).toBe(false);
  });

  it("git-pr.json matches the zod schema", () => {
    const parsed = prResultSchema.parse(fixture("git-pr.json"));
    expect(parsed.url).toBe("https://github.com/acme/grove/pull/42");
    expect(parsed.draft).toBe(true);
  });

  it("rejects a status payload that drifted from the Rust shape", () => {
    const raw = fixture("git-status.json") as Record<string, unknown>;
    expect(() => gitStatusSchema.parse({ ...raw, truncated: "no" })).toThrow();
    expect(() => gitStatusSchema.parse({ ...raw, files: [{ path: "a" }] })).toThrow();
  });
});

describe("grove://git-changed", () => {
  it("parses the fixture envelope and its status payload", () => {
    const parsed = parseGitChanged(fixture("git-changed.json"));
    expect(parsed?.payload.branch).toBe("ticket/04-git-panel");
    expect(parsed?.payload.files).toHaveLength(4);
  });

  it("ignores a malformed payload instead of throwing", () => {
    expect(parseGitChanged({ payload: { repoRoot: 1 } })).toBeNull();
    expect(parseGitChanged({ nope: true })).toBeNull();
    expect(parseGitChanged(null)).toBeNull();
  });
});
