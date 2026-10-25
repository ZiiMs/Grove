import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "./App";
import type {
  GitBranch,
  GitWorktree,
  SessionEvent,
  Project,
  SessionDetail,
} from "@grove/contract";
import { BridgeProvider } from "./ipc/context";
import { DEFAULT_PROVIDERS, createFakeBridge, fakeSession, fakeSummary } from "./test-utils/fakeBridge";
import { useSessionStore } from "./state/sessionStore";
import { useWorkspaceStore } from "./state/workspaceStore";
import { useContextStore } from "./state/contextStore";
import { useUiStore } from "./state/uiStore";

const project: Project = {
  id: "project-1",
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

const branch: GitBranch = {
  name: "feat/ui",
  remote: null,
  current: true,
  upstream: null,
  ahead: 2,
  behind: 0,
};

const session = fakeSession({
  id: "session-1",
  projectId: project.id,
  provider: "claude",
  model: "claude-sonnet-4-5",
  title: "OAuth device flow",
  status: "idle",
  awaiting: true,
  cwd: worktree.path,
});

const events: SessionEvent[] = [
  {
    at: "2025-01-01T00:00:01.000Z",
    kind: "message",
    role: "user",
    blocks: [{ type: "text", text: "Wire up the GitHub OAuth device flow." }],
  },
  {
    at: "2025-01-01T00:00:02.000Z",
    kind: "message",
    role: "assistant",
    blocks: [{ type: "text", text: "Reading the session module first." }],
  },
  {
    at: "2025-01-01T00:00:03.000Z",
    kind: "tool_call",
    call_id: "call-1",
    name: "Read",
    title: "apps/desktop/src/session.rs",
  },
  {
    at: "2025-01-01T00:00:03.500Z",
    kind: "tool_result",
    call_id: "call-1",
    status: "ok",
    summary: "214 lines read",
  },
  {
    at: "2025-01-01T00:00:04.000Z",
    kind: "permission_request",
    request_id: "perm-1",
    title: "cargo test -p grove-auth oauth",
    detail: "Runs the auth crate's test suite.",
  },
];

const detail: SessionDetail = { ...session, events };

function seed() {
  return {
    providers: DEFAULT_PROVIDERS,
    sessions: [fakeSummary({ ...session, awaiting: true }, events.length)],
    details: { [session.id]: detail },
    projects: [project],
    worktrees: [worktree],
    branches: [branch],
    gitStatus: {
      repoRoot: worktree.path,
      branch: "feat/ui",
      head: worktree.head,
      clean: false,
      truncated: false,
      files: [
        { path: "apps/desktop/src/session.rs", from: null, index: "M", worktree: " ", staged: true, untracked: false },
      ],
    },
    installs: [],
    plans: {
      [session.id]: {
        sessionId: session.id,
        markdown: [
          "# Plan",
          "",
          "- [x] Provider registry",
          "- [x] Codex adapter",
          "- [x] ACP handshake",
          "- [x] Session tabs",
          "- [ ] Permission gate inside the transcript",
          "- [ ] Read-only diff inspector",
          "- [ ] Persist session state across restart",
        ].join("\n"),
        version: 2,
        mirrorPath: `${project.path}/.grove/sessions/${session.id}/plan.md`,
      },
    },
    specGraph: {
      root: project.path,
      nodes: [
        {
          id: "0007-provider-registry",
          type: "architecture-design",
          title: "0007-provider-registry",
          status: "Accepted",
          path: "specs/0007-provider-registry.md",
          dependsOn: [],
          references: [],
          implements: [],
          covers: [],
        },
      ],
      validation: { danglingLinks: [], duplicateIds: [], parentCycles: [] },
    },
    specTitles: { "0007-provider-registry": "0007-provider-registry" },
  };
}

function renderApp() {
  const bridge = createFakeBridge({}, seed());
  render(
    <BridgeProvider bridge={bridge}>
      <App />
    </BridgeProvider>,
  );
  return bridge;
}

async function ready() {
  const bridge = renderApp();
  await screen.findByRole("heading", { name: "OAuth device flow" });
  return bridge;
}

beforeEach(() => {
  localStorage.clear();
  useSessionStore.getState().reset();
  useWorkspaceStore.getState().reset();
  useContextStore.getState().reset();
  useUiStore.getState().reset();
});

describe("redesigned shell", () => {
  it("renders the project tree, the active session and the status line", async () => {
    await ready();
    expect(screen.getByText("grove")).toBeInTheDocument();
    expect(screen.getAllByText("feat/ui").length).toBeGreaterThan(0);
    expect(screen.getByText(/grove 0\.1\.0/)).toBeInTheDocument();
    expect(screen.getAllByText(/1 awaiting you/).length).toBeGreaterThan(0);
    expect(screen.getByText(/Wire up the GitHub OAuth device flow/)).toBeInTheDocument();
    expect(screen.getByText("Read")).toBeInTheDocument();
  });

  it("opens settings on the requested tab and applies a theme", async () => {
    await ready();
    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    await userEvent.click(screen.getByRole("tab", { name: /Theme/ }));
    await userEvent.click(screen.getByRole("button", { name: /Terminal Native/ }));
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("terminal"));
    expect(localStorage.getItem("grove.theme")).toContain("terminal");
  });

  it("creates a session from the new-session dialog", async () => {
    const bridge = await ready();
    await userEvent.click(screen.getByRole("button", { name: /^New session$/ }));
    const title = screen.getByLabelText("What should this session work on?");
    await userEvent.type(title, "Add the session tabs");
    await userEvent.click(screen.getByRole("button", { name: "Start session" }));
    await screen.findByRole("heading", { name: "Add the session tabs" });
    expect(bridge.invocations).toContain("session_create");
  });

  it("answers a permission request from the transcript", async () => {
    const bridge = await ready();
    await userEvent.click(screen.getByRole("button", { name: "Approve and run" }));
    await waitFor(() => expect(bridge.invocations).toContain("session_respond_permission"));
    await screen.findByText("Approved");
  });

  it("shows the plan checklist and pins a spec from the context panel", async () => {
    const bridge = await ready();

    await userEvent.click(screen.getByRole("tab", { name: /Plan/ }));
    expect(await screen.findByText("4 of 7 complete")).toBeInTheDocument();
    expect(screen.getByText("Permission gate inside the transcript")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: "Spec" }));
    await userEvent.click(await screen.findByRole("button", { name: /0007-provider-registry/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Attach to session" }));

    await waitFor(() =>
      expect(bridge.calls.some((call) => call.command === "session_pin")).toBe(true),
    );
    expect(bridge.invocations).toContain("session_refresh_context");
    expect(await screen.findByText("0007-provider-registry")).toBeInTheDocument();
  });

  it("creates a worktree and starts the session in it", async () => {
    const bridge = await ready();
    await userEvent.click(screen.getByRole("button", { name: "New session in grove" }));
    await userEvent.type(
      screen.getByLabelText("What should this session work on?"),
      "Add session tabs",
    );
    expect(screen.getByDisplayValue("grove/add-session-tabs")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Start session" }));

    await screen.findByRole("heading", { name: "Add session tabs" });
    const worktreeCall = bridge.calls.find((call) => call.command === "git_worktree_create");
    expect(worktreeCall?.args).toMatchObject({
      repoPath: project.path,
      branch: "grove/add-session-tabs",
      base: "feat/ui",
    });
    const createCall = bridge.calls.find((call) => call.command === "session_create");
    expect(createCall?.args).toMatchObject({ req: { projectId: project.id } });
  });

  it("queues a message while a turn runs and sends it when the session goes idle", async () => {
    const bridge = await ready();
    const composer = screen.getByLabelText("Message the active session");
    await userEvent.type(composer, "Then update the changelog");
    await userEvent.click(screen.getByRole("button", { name: "Queue this message for the next turn" }));
    expect(await screen.findByText("queued")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Approve and run" }));
    await waitFor(() => expect(useSessionStore.getState().detail?.awaiting).toBe(false));

    act(() => {
      bridge.emitSessionEvent({
        sessionId: session.id,
        payload: { at: "2025-01-01T00:00:09.000Z", kind: "status", state: "idle" },
      });
    });

    await waitFor(() =>
      expect(
        bridge.calls.some(
          (call) =>
            call.command === "session_send" &&
            (call.args as { text?: string }).text === "Then update the changelog",
        ),
      ).toBe(true),
    );
  });

  it("adds a folder as a project from the picker", async () => {
    const bridge = await ready();
    await userEvent.click(screen.getByRole("button", { name: /Add project/ }));
    const dialog = screen.getByRole("dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Choose folder" }));
    await screen.findByText("C:/Users/demo");
    await userEvent.click(within(dialog).getByRole("button", { name: "Use this folder" }));
    await userEvent.click(within(dialog).getByRole("button", { name: "Add project" }));

    await waitFor(() =>
      expect(bridge.calls.some((call) => call.command === "project_add")).toBe(true),
    );
    expect(await screen.findByText("demo")).toBeInTheDocument();
  });
});
