import { describe, expect, it } from "vitest";
import type { Project } from "@grove/contract";
import {
  basename,
  dirname,
  normalizeFolderPath,
  normalizePath,
  projectForSession,
  projectNameFromPath,
  worktreeForSession,
} from "./tree";
import { fakeSession, fakeSummary } from "../test-utils/fakeBridge";

const project: Project = {
  id: "p1",
  name: "grove",
  path: "C:/code/grove",
  remote: null,
  defaultBranch: "main",
  isGit: true,
  createdAt: "2025-01-01T00:00:00.000Z",
  updatedAt: "2025-01-01T00:00:00.000Z",
};

describe("tree mapping", () => {
  it("normalises separators and trailing slashes", () => {
    expect(normalizePath("C:\\Code\\Grove\\")).toBe("c:/code/grove");
  });

  it("uses projectId first, then falls back to the cwd prefix", () => {
    const linked = fakeSummary(fakeSession({ projectId: "p1", cwd: "C:/elsewhere" }));
    expect(projectForSession(linked, [project])?.id).toBe("p1");

    const unlinked = fakeSummary(fakeSession({ projectId: null, cwd: "C:/code/grove/src" }));
    expect(projectForSession(unlinked, [project])?.id).toBe("p1");

    const unrelated = fakeSummary(fakeSession({ projectId: null, cwd: "C:/other/repo" }));
    expect(projectForSession(unrelated, [project])).toBeNull();
  });

  it("picks the deepest matching worktree", () => {
    const session = fakeSummary(fakeSession({ cwd: "C:/code/grove-wt/feature/src" }));
    const worktrees = [
      { path: "C:/code/grove-wt" },
      { path: "C:/code/grove-wt/feature" },
      { path: "C:/code/other" },
    ];
    expect(worktreeForSession(session, worktrees)?.path).toBe("C:/code/grove-wt/feature");
  });

  it("splits names and folders on either separator", () => {
    expect(basename("C:\\code\\grove\\session.rs")).toBe("session.rs");
    expect(dirname("C:/code/grove/session.rs")).toBe("C:/code/grove");
    expect(dirname("session.rs")).toBe("");
  });

  it("prettifies a folder name for the project default", () => {
    expect(projectNameFromPath("D:/Dev/grove")).toBe("Grove");
    expect(projectNameFromPath("D:\\Dev\\my-project")).toBe("My Project");
    expect(projectNameFromPath("C:/code/API_client")).toBe("API Client");
    expect(projectNameFromPath("C:/code/Grove")).toBe("Grove");
  });

  it("normalises a pasted folder path for the picker", () => {
    for (const raw of ["D:\\Dev\\grove\\", "D:/Dev/grove", "  \"D:/Dev/grove\"  "]) {
      expect(normalizeFolderPath(raw)).toBe("D:/Dev/grove");
    }
    expect(normalizeFolderPath("D:\\")).toBe("D:/");
    expect(normalizeFolderPath("/")).toBe("/");
    expect(normalizeFolderPath("//server/share/")).toBe("//server/share");
    expect(normalizeFolderPath("~\\dev\\grove")).toBe("~/dev/grove");
  });
});
