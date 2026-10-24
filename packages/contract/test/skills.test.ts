import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  installResultSchema,
  skillCatalogSchema,
  skillPreviewSchema,
  skillTargetsSchema,
} from "../src/index";

function fixture(name: string): unknown {
  const path = fileURLToPath(new URL(`../../../contract/fixtures/${name}`, import.meta.url));
  return JSON.parse(readFileSync(path, "utf8"));
}

describe("skill contract fixtures", () => {
  it("skill-catalog.json matches the zod schema", () => {
    const parsed = skillCatalogSchema.parse(fixture("skill-catalog.json"));
    expect(parsed.entries).toHaveLength(2);
    expect(parsed.entries[0].id).toBe("anthropics/skills::skills/pdf");
    expect(parsed.fromCache).toBe(false);
    expect(parsed.offline).toBe(false);
  });

  it("skill-preview.json matches the zod schema", () => {
    const parsed = skillPreviewSchema.parse(fixture("skill-preview.json"));
    expect(parsed.entry.path).toBe("skills/pdf");
    expect(parsed.frontmatter.description).toBe("Create and inspect PDF files");
    expect(parsed.body).toContain("# PDF");
  });

  it("skill-targets.json matches the zod schema", () => {
    const parsed = skillTargetsSchema.parse(fixture("skill-targets.json"));
    expect(parsed.map((target) => target.id)).toEqual([
      "claude",
      "codex",
      "pi",
      "opencode",
      "agents",
    ]);
    expect(parsed[4].exists).toBe(true);
  });

  it("skill-install.json matches the zod schema", () => {
    const parsed = installResultSchema.parse(fixture("skill-install.json"));
    expect(parsed.overwritten).toBe(true);
    expect(parsed.path).toBe("C:/Users/demo/.claude/skills/pdf");
  });

  it("rejects a catalog entry without a manifest path", () => {
    expect(() =>
      skillCatalogSchema.parse({
        entries: [{ id: "x", name: "x", source: "x" }],
        fetchedAt: null,
        fromCache: false,
        offline: false,
      }),
    ).toThrow();
  });
});
