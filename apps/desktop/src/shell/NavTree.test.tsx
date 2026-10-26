import { cleanup, render, screen } from "@testing-library/react";
import type { ComponentProps } from "react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GitWorktree, Project, SessionSummary } from "@grove/contract";
import { NavTree } from "./NavTree";
import { useSessionStore } from "../state/sessionStore";
import { useWorkspaceStore } from "../state/workspaceStore";
import { fakeSession, fakeSummary } from "../test-utils/fakeBridge";

const project: Project = {
  id: "p1",
  name: "grove",
  path: "C:/code/grove",
  remote: "acme/grove",
  defaultBranch: "main",
  isGit: true,
  createdAt: "2025-01-01T00:00:00.000Z",
  updatedAt: "2025-01-01T00:00:00.000Z",
};

const worktree: GitWorktree = {
  path: "C:/code/grove",
  head: "1111111111111111111111111111111111111111",
  branch: "feat/ui",
  bare: false,
  detached: false,
  current: true,
};

const linked = fakeSummary(
  fakeSession({ id: "s1", projectId: "p1", cwd: worktree.path, title: "OAuth device flow", awaiting: true }),
);
const loose = fakeSummary(fakeSession({ id: "s2", projectId: null, cwd: "C:/elsewhere", title: "ACP notes" }));

function renderTree(overrides: Partial<ComponentProps<typeof NavTree>> = {}) {
  const props = {
    active: true,
    filter: "",
    onFilter: vi.fn(),
    activeId: "s1",
    onSelect: vi.fn(),
    onSelectSubagent: vi.fn(),
    onNewSession: vi.fn(),
    onNewChat: vi.fn(),
    onAddProject: vi.fn(),
    onSettings: vi.fn(),
    ...overrides,
  };
  render(<NavTree {...props} />);
  return props;
}

beforeEach(() => {
  useSessionStore.getState().reset();
  useWorkspaceStore.getState().reset();
  useSessionStore.setState({ sessions: [linked, loose], activeId: "s1" });
  useWorkspaceStore.setState({
    projects: [project],
    worktrees: { p1: [worktree] },
    branches: {},
    gitErrors: {},
  });
});

describe("NavTree", () => {
  it("groups sessions under chats, projects, worktrees and sessions", () => {
    renderTree();
    expect(screen.getByText("Chats")).toBeInTheDocument();
    expect(screen.getByText("ACP notes")).toBeInTheDocument();
    expect(screen.getByText("grove")).toBeInTheDocument();
    expect(screen.getByText("feat/ui")).toBeInTheDocument();
    expect(screen.getByText("OAuth device flow")).toBeInTheDocument();
    expect(screen.getByText("2 sessions · 1 awaiting you")).toBeInTheDocument();
  });

  it("reports filter input to the shell", async () => {
    const props = renderTree();
    await userEvent.type(screen.getByLabelText(/Filter projects/), "oauth");
    expect(props.onFilter).toHaveBeenCalled();
  });

  it("shows only matching sessions while filtering", () => {
    renderTree({ filter: "oauth" });
    expect(screen.getByText("OAuth device flow")).toBeInTheDocument();
    expect(screen.queryByText("ACP notes")).not.toBeInTheDocument();
    cleanup();
  });

  it("collapses a project without losing its count", async () => {
    renderTree();
    await userEvent.click(screen.getByRole("button", { name: "Toggle grove" }));
    expect(document.getElementById("kids-p-p1")).toHaveAttribute("hidden");
    expect(screen.getByRole("button", { name: "Toggle grove" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  it("asks the shell to open a new session on the project or its worktree", async () => {
    const props = renderTree();
    await userEvent.click(screen.getByRole("button", { name: "New session in grove" }));
    expect(props.onNewSession).toHaveBeenCalledWith("p1", null);
    await userEvent.click(screen.getByRole("button", { name: "New session on feat/ui" }));
    expect(props.onNewSession).toHaveBeenCalledWith("p1", worktree.path);
  });

  it("selects a session row", async () => {
    const props = renderTree();
    await userEvent.click(screen.getByRole("button", { name: /OAuth device flow/ }));
    expect(props.onSelect).toHaveBeenCalledWith("s1");
  });

  it("shows subagents under their session and selects the parent", async () => {
    const delegated: SessionSummary = {
      ...fakeSummary(
        fakeSession({ id: "s3", projectId: "p1", cwd: worktree.path, title: "Store audit" }),
      ),
      subagents: [
        {
          callId: "call-1",
          description: "read the store",
          status: "ok",
          summary: "the report",
          startedAt: "2025-01-01T00:00:00.000Z",
          finishedAt: "2025-01-01T00:00:05.000Z",
        },
      ],
    };
    useSessionStore.setState({ sessions: [delegated], activeId: "s3" });
    const props = renderTree({ activeId: "s3" });

    expect(screen.getByText("read the store")).toBeInTheDocument();
    expect(screen.getByTitle("1 subagent")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /read the store/ }));
    expect(props.onSelectSubagent).toHaveBeenCalledWith("s3", "call-1");
  });
});
