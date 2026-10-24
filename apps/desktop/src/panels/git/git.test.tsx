import type { GitBranch, GitDiff, GitStatus, GitWorktree } from "@grove/contract";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { BridgeProvider } from "../../ipc/context";
import { createFakeBridge, type FakeBridge } from "../../test-utils/fakeBridge";
import { GitPanel } from "./index";

/**
 * Monaco is mocked and counted: the panel must not request the module on mount, only when a diff is
 * opened. The counter lives in a hoisted object because `vi.mock` factories are lifted above imports.
 */
const monaco = vi.hoisted(() => ({ loads: 0 }));

vi.mock("@monaco-editor/react", async () => {
  const react = await import("react");
  monaco.loads += 1;
  return {
    Editor: ({ value }: { value?: string }) =>
      react.createElement("pre", { "data-testid": "monaco-diff" }, value ?? ""),
  };
});

const HEAD = "9f2c1b7d4e5a6c8b0d1e2f3a4b5c6d7e8f901234";

const dirtyStatus: GitStatus = {
  repoRoot: "C:/work/grove",
  branch: "ticket/04-git-panel",
  head: HEAD,
  clean: false,
  truncated: false,
  files: [
    {
      path: "src/main.ts",
      from: null,
      index: "M",
      worktree: " ",
      staged: true,
      untracked: false,
    },
    {
      path: "scratch.txt",
      from: null,
      index: "?",
      worktree: "?",
      staged: false,
      untracked: true,
    },
  ],
};

const cleanStatus: GitStatus = {
  ...dirtyStatus,
  head: "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678",
  clean: true,
  files: [],
};

const branches: GitBranch[] = [
  {
    name: "ticket/04-git-panel",
    remote: null,
    current: true,
    upstream: "origin/ticket/04-git-panel",
    ahead: 2,
    behind: 1,
  },
  {
    name: "origin/main",
    remote: "origin",
    current: false,
    upstream: null,
    ahead: 0,
    behind: 0,
  },
];

const worktrees: GitWorktree[] = [
  {
    path: "C:/work/grove",
    head: HEAD,
    branch: "ticket/04-git-panel",
    bare: false,
    detached: false,
    current: true,
  },
  {
    path: "C:/work/grove-wt",
    head: HEAD,
    branch: "wt-branch",
    bare: false,
    detached: false,
    current: false,
  },
];

const diff: GitDiff = {
  repoRoot: "C:/work/grove",
  target: "working_tree",
  path: "scratch.txt",
  base: null,
  branch: null,
  patch: "diff --git a/scratch.txt b/scratch.txt\n+scratch\n",
  truncated: false,
};

function renderPanel(configure: (bridge: FakeBridge) => void) {
  const bridge = createFakeBridge();
  bridge.on("git_status", () => dirtyStatus);
  bridge.on("git_branches", () => branches);
  bridge.on("git_worktrees", () => worktrees);
  configure(bridge);
  render(
    <BridgeProvider bridge={bridge}>
      <GitPanel />
    </BridgeProvider>,
  );
  return bridge;
}

describe("GitPanel", () => {
  it("loads status, opens a diff lazily, and commits the selected paths", async () => {
    const commitCalls: unknown[] = [];
    const diffCalls: unknown[] = [];
    const bridge = renderPanel((fake) => {
      fake.on("git_diff", (args) => {
        diffCalls.push(args);
        return diff;
      });
      fake.on("git_commit", (args) => {
        commitCalls.push(args);
        return cleanStatus;
      });
    });
    const loadsBefore = monaco.loads;

    // Status and branch/worktree lists come from the bridge, not from a hard-coded placeholder.
    expect(await screen.findByText(/Current branch/)).toBeInTheDocument();
    const branchList = within(screen.getByRole("region", { name: "Branches" }));
    expect(branchList.getByText("ticket/04-git-panel")).toBeInTheDocument();
    expect(branchList.getByText("origin/main")).toBeInTheDocument();
    expect(
      within(screen.getByRole("region", { name: "Worktrees" })).getByText(/C:\/work\/grove-wt/),
    ).toBeInTheDocument();
    expect(monaco.loads).toBe(loadsBefore);

    await userEvent.click(screen.getByRole("button", { name: "View diff for scratch.txt" }));
    const viewer = await screen.findByTestId("monaco-diff");
    expect(viewer).toHaveTextContent("+scratch");
    expect(monaco.loads).toBe(loadsBefore + 1);
    expect(diffCalls[0]).toMatchObject({ path: "scratch.txt" });

    await userEvent.click(screen.getByRole("checkbox", { name: "Select scratch.txt" }));
    await userEvent.type(screen.getByRole("textbox", { name: "Commit message" }), "add scratch");
    await userEvent.click(screen.getByRole("button", { name: "Commit selected paths" }));

    await waitFor(() => expect(commitCalls).toHaveLength(1));
    expect(commitCalls[0]).toMatchObject({ message: "add scratch", paths: ["scratch.txt"] });
    expect(await screen.findByText(/Committed a1b2c3d on ticket\/04-git-panel/)).toBeInTheDocument();
    expect(bridge.invocations).toContain("git_commit");
  });

  it("surfaces the reason when git itself is unavailable", async () => {
    renderPanel((fake) => {
      fake.on("git_status", () =>
        Promise.reject({ code: "unavailable", message: "git is not installed or not on PATH" }),
      );
    });

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("unavailable");
    expect(alert).toHaveTextContent("git is not installed or not on PATH");
  });

  it("surfaces the gh unavailable reason beside the PR form", async () => {
    renderPanel((fake) => {
      fake.on("git_open_pr", () =>
        Promise.reject({
          code: "unavailable",
          message: "gh is not authenticated; run `gh auth login`",
          detail: "To get started with GitHub CLI, please run: gh auth login",
        }),
      );
    });

    await userEvent.type(await screen.findByRole("textbox", { name: "PR title" }), "Add git panel");
    await userEvent.click(screen.getByRole("button", { name: "Open draft PR" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("gh is not authenticated");
    expect(alert).toHaveTextContent("gh auth login");
  });

  it("shows the PR URL when gh succeeds", async () => {
    renderPanel((fake) => {
      fake.on("git_open_pr", () => ({
        url: "https://github.com/acme/grove/pull/42",
        draft: true,
        base: "main",
      }));
    });

    await userEvent.type(await screen.findByRole("textbox", { name: "PR title" }), "Add git panel");
    await userEvent.click(screen.getByRole("button", { name: "Open draft PR" }));

    const link = await screen.findByRole("link", { name: "https://github.com/acme/grove/pull/42" });
    expect(link).toHaveAttribute("href", "https://github.com/acme/grove/pull/42");
  });

  it("refreshes the status when the core emits grove://git-changed", async () => {
    const bridge = renderPanel(() => {});
    expect(await screen.findByText(/2 changed/)).toBeInTheDocument();

    act(() => {
      bridge.emit("grove://git-changed", { payload: cleanStatus });
    });

    expect(await screen.findByText(/clean/)).toBeInTheDocument();
    expect(screen.queryByText(/scratch\.txt/)).not.toBeInTheDocument();
  });
});
