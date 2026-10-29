import { describe, expect, it } from "vitest";
import { generateWorktreeName } from "./worktreeName";

describe("generateWorktreeName", () => {
  it("builds a lowercase adjective-plant name", () => {
    expect(generateWorktreeName(new Set(), () => 0)).toBe("quiet-cedar");
  });

  it("skips a name that is already taken", () => {
    const name = generateWorktreeName(new Set(["quiet-cedar"]), () => 0);
    expect(name).not.toBe("quiet-cedar");
    expect(name).toMatch(/^[a-z]+-[a-z]+$/);
  });

  it("keeps generating different names from different random positions", () => {
    const names = new Set([
      generateWorktreeName(new Set(), () => 0),
      generateWorktreeName(new Set(), () => 0.25),
      generateWorktreeName(new Set(), () => 0.5),
      generateWorktreeName(new Set(), () => 0.75),
    ]);
    expect(names.size).toBe(4);
  });
});
