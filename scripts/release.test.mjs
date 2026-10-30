import { describe, expect, test } from "bun:test";
import {
  bumpVersion,
  cargoLockVersion,
  cargoWorkspaceVersion,
  changelogSection,
  parseCommit,
  renderNotes,
  upsertChangelog,
} from "./release.mjs";

describe("parseCommit", () => {
  test("reads type, scope and description", () => {
    expect(parseCommit("feat(session): close archive races")).toEqual({
      type: "feat",
      scope: "session",
      description: "close archive races",
      breaking: false,
    });
  });

  test("marks a bang as breaking", () => {
    expect(parseCommit("feat(api)!: drop the v1 endpoint").breaking).toBe(true);
  });

  test("marks a BREAKING CHANGE footer as breaking", () => {
    expect(
      parseCommit("fix(git): rename the field", "BREAKING CHANGE: the old field is gone").breaking,
    ).toBe(true);
  });

  test("drops merge and release commits", () => {
    expect(parseCommit("Merge pull request #64 from ZiiMs/feat/x")).toBeNull();
    expect(parseCommit("chore(release): v0.3.1 [skip ci]")).toBeNull();
    expect(parseCommit("chore: release v0.2.0")).toBeNull();
  });

  test("keeps a non-conventional subject under other", () => {
    expect(parseCommit("Initial commit")).toEqual({
      type: "other",
      scope: null,
      description: "Initial commit",
      breaking: false,
    });
  });
});

describe("bumpVersion", () => {
  const feat = { type: "feat", breaking: false };
  const fix = { type: "fix", breaking: false };
  const breaking = { type: "feat", breaking: true };

  test("pre-1.0 features move the minor", () => {
    expect(bumpVersion("0.3.0", [feat])).toBe("0.4.0");
  });

  test("pre-1.0 breaking changes move the minor", () => {
    expect(bumpVersion("0.3.2", [breaking])).toBe("0.4.0");
  });

  test("fixes move the patch", () => {
    expect(bumpVersion("0.3.2", [fix])).toBe("0.3.3");
  });

  test("1.0 breaking changes move the major", () => {
    expect(bumpVersion("1.2.3", [breaking])).toBe("2.0.0");
  });

  test("1.0 features move the minor", () => {
    expect(bumpVersion("1.2.3", [feat])).toBe("1.3.0");
  });
});

describe("renderNotes", () => {
  test("groups by type, scopes the lines and skips release noise", () => {
    const notes = renderNotes([
      { type: "feat", scope: "git", description: "archive sessions", breaking: false },
      { type: "fix", scope: null, description: "keep the toast", breaking: false },
      { type: "docs", scope: null, description: "record the release", breaking: false },
    ]);
    expect(notes).toContain("### Added\n\n- **git:** archive sessions");
    expect(notes).toContain("### Fixed\n\n- keep the toast");
    expect(notes).toContain("### Docs\n\n- record the release");
  });

  test("lists breaking changes separately", () => {
    const notes = renderNotes([
      { type: "feat", scope: "api", description: "drop v1", breaking: true },
    ]);
    expect(notes).toContain("### Breaking changes\n\n- **api:** drop v1");
    expect(notes).not.toContain("### Added");
  });
});

describe("upsertChangelog", () => {
  const date = new Date("2026-10-30T12:00:00Z");
  const first = changelogSection("0.3.1", "### Fixed\n\n- one", date);

  test("starts a new file with the header", () => {
    const file = upsertChangelog("", first);
    expect(file.startsWith("# Changelog")).toBe(true);
    expect(file).toContain("## v0.3.1 — 2026-10-30");
  });

  test("prepends a newer version", () => {
    const older = changelogSection("0.3.0", "### Added\n\n- zero", date);
    const file = upsertChangelog(upsertChangelog("", older), first);
    expect(file.indexOf("## v0.3.1")).toBeLessThan(file.indexOf("## v0.3.0"));
  });

  test("replaces the same version instead of duplicating it", () => {
    const file = upsertChangelog(upsertChangelog("", first), first);
    expect(file.match(/## v0\.3\.1/g)?.length).toBe(1);
  });

  test("does not confuse 0.3.1 with 0.3.10", () => {
    const ten = changelogSection("0.3.10", "### Added\n\n- ten", date);
    const one = changelogSection("0.3.1", "### Added\n\n- one", date);
    const file = upsertChangelog(upsertChangelog("", ten), one);
    expect(file.match(/## v0\.3\.10/g)?.length).toBe(1);
    expect(file.match(/## v0\.3\.1 /g)?.length).toBe(1);
  });
});

describe("cargoWorkspaceVersion", () => {
  test("reads the workspace package version only", () => {
    const text = [
      "[workspace.package]",
      'version = "0.3.0"',
      "",
      "[profile.dev]",
      'opt-level = 0',
    ].join("\n");
    expect(cargoWorkspaceVersion(text)).toBe("0.3.0");
  });
});

describe("cargoLockVersion", () => {
  test("reads the named workspace package", () => {
    const text = [
      "[[package]]",
      'name = "grove-core"',
      'version = "0.3.0"',
      "",
      "[[package]]",
      'name = "other"',
      'version = "9.9.9"',
    ].join("\n");
    expect(cargoLockVersion(text, "grove-core")).toBe("0.3.0");
    expect(cargoLockVersion(text, "grove-desktop")).toBeNull();
  });
});
