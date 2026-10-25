import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  addProjectRequestSchema,
  dirListingSchema,
  projectDetectionSchema,
  projectSchema,
} from "../src/index";

function fixture(name: string): unknown {
  const path = fileURLToPath(new URL(`../../../contract/fixtures/${name}`, import.meta.url));
  return JSON.parse(readFileSync(path, "utf8"));
}

describe("workspace contract fixtures", () => {
  it("project.json matches the zod schema", () => {
    const parsed = projectSchema.parse(fixture("project.json"));
    expect(parsed.id).toBe("project-1");
    expect(parsed.isGit).toBe(true);
    expect(parsed.defaultBranch).toBe("main");
  });

  it("project-detection.json carries git facts before a project is saved", () => {
    const parsed = projectDetectionSchema.parse(fixture("project-detection.json"));
    expect(parsed.currentBranch).toBe("feat/ui");
    expect(parsed.branches).toEqual(["feat/ui", "main"]);
  });

  it("dir-listing.json lists one level of folders", () => {
    const parsed = dirListingSchema.parse(fixture("dir-listing.json"));
    expect(parsed.parent).toBe("C:/Users/ada");
    expect(parsed.dirs.map((dir) => dir.name)).toEqual(["agent-hub", "grove"]);
  });

  it("project_add accepts a path alone and keeps optional fields nullable", () => {
    const parsed = addProjectRequestSchema.parse({ path: "C:/work/grove" });
    expect(parsed.name).toBeUndefined();
    expect(() => addProjectRequestSchema.parse({})).toThrow();
  });
});
