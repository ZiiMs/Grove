import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { fsRootsSchema } from "@grove/contract";
import type {
  DirListing,
  GitBranch,
  GitWorktree,
  ProviderSpec,
  SessionEvent,
  Project,
  SessionDetail,
} from "@grove/contract";
import fsRootsFixture from "../../../contract/fixtures/fs-roots.json";
import { BridgeProvider } from "./ipc/context";
import {
  DEFAULT_PROVIDERS,
  FAKE_PROVIDER_ROSTER,
  MOCK_CAPABILITIES,
  createFakeBridge,
  fakeProviderStatus,
  fakeSession,
  fakeSummary,
  type FakeBridgeSeed,
} from "./test-utils/fakeBridge";
import { useSessionStore } from "./state/sessionStore";
import { useWorkspaceStore } from "./state/workspaceStore";
import { useContextStore } from "./state/contextStore";
import { useMenuStore } from "./state/menuStore";
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

/** The shipped roster, with Anthropic and OpenAI connected and the two gateways not. */
const PROVIDER_ROSTER: ProviderSpec[] = [
  ...FAKE_PROVIDER_ROSTER.map((row) => ({
    id: row.id,
    name: row.name,
    short: row.short,
    tint: row.tint,
    wire: "open_ai" as const,
    baseUrl: "https://example.invalid",
    auth: "api_key" as const,
    keyUrl: row.keyUrl,
    envVar: row.envVar,
    models: row.models,
    defaultModel: row.defaultModel,
    capabilities: MOCK_CAPABILITIES,
  })),
  ...DEFAULT_PROVIDERS,
];

const anthropicSession = { ...session, provider: "anthropic", model: "claude-sonnet-4-5" };
const anthropicDetail: SessionDetail = {
  ...detail,
  provider: "anthropic",
  model: "claude-sonnet-4-5",
};

/** The same conversation once its permission request has been answered. */
const idleEvents = events.filter((event) => event.kind !== "permission_request");
const idleSession = { ...session, awaiting: false };
const idleDetail: SessionDetail = { ...idleSession, events: idleEvents };
const idleAnthropicSession = { ...anthropicSession, awaiting: false };
const idleAnthropicDetail: SessionDetail = { ...idleDetail, provider: "anthropic", model: "claude-sonnet-4-5" };

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

function renderApp(overrides: FakeBridgeSeed = {}) {
  const bridge = createFakeBridge({}, { ...seed(), ...overrides });
  render(
    <BridgeProvider bridge={bridge}>
      <App />
    </BridgeProvider>,
  );
  return bridge;
}

const FS_ROOTS = fsRootsSchema.parse(fsRootsFixture);

function folder(path: string, parent: string | null, names: string[]): DirListing {
  const base = path.replace(/\/+$/, "");
  return {
    path,
    parent,
    dirs: names.map((name) => ({ name, path: `${base}/${name}` })),
  };
}

/** A picker with a home folder on C: and a project on D:, as a second drive would be. */
function pickerSeed(): FakeBridgeSeed {
  return {
    fsRoots: FS_ROOTS,
    fsListings: {
      "C:/Users/ada": folder("C:/Users/ada", "C:/", ["code", "notes"]),
      "C:/Users/ada/code": folder("C:/Users/ada/code", "C:/Users/ada", ["agent-hub", "grove"]),
      "C:/Users/ada/code/grove": folder("C:/Users/ada/code/grove", "C:/Users/ada/code", ["src"]),
      "C:/": folder("C:/", null, ["Users"]),
      "D:/": folder("D:/", null, ["Dev"]),
      "D:/Dev": folder("D:/Dev", "D:/", ["grove"]),
      "D:/Dev/grove": folder("D:/Dev/grove", "D:/Dev", ["docs", "src"]),
    },
    detections: {
      "D:/Dev/grove": {
        path: "D:/Dev/grove",
        isGit: true,
        currentBranch: "main",
        branches: ["main", "feat/folder-picker"],
        defaultBranch: "main",
        remote: "acme/grove",
      },
    },
  };
}

async function openPicker() {
  const bridge = renderApp(pickerSeed());
  await screen.findByRole("heading", { name: "OAuth device flow" });
  await userEvent.click(screen.getByRole("button", { name: /Add project/ }));
  const dialog = screen.getByRole("dialog");
  await userEvent.click(within(dialog).getByRole("button", { name: "Choose folder" }));
  const address = await within(dialog).findByLabelText("Folder path");
  await within(dialog).findByRole("option", { name: "code" });
  return { bridge, dialog, address };
}

async function ready() {
  const bridge = renderApp();
  await screen.findByRole("heading", { name: "OAuth device flow" });
  return bridge;
}

/** The shell with the full provider roster on an idle Anthropic session. */
async function readyOnAnthropic() {
  const bridge = renderApp({
    providers: PROVIDER_ROSTER,
    providerStatuses: FAKE_PROVIDER_ROSTER,
    sessions: [fakeSummary(anthropicSession, events.length)],
    details: { [session.id]: anthropicDetail },
  });
  await screen.findByRole("heading", { name: "OAuth device flow" });
  return bridge;
}

/** The same session, idle: the permission request has been answered. */
async function readyOnIdleAnthropic() {
  const bridge = renderApp({
    providers: PROVIDER_ROSTER,
    providerStatuses: FAKE_PROVIDER_ROSTER,
    sessions: [fakeSummary(idleAnthropicSession, idleEvents.length)],
    details: { [session.id]: idleAnthropicDetail },
  });
  await screen.findByRole("heading", { name: "OAuth device flow" });
  return bridge;
}

/** The default shell session, idle. */
async function readyIdle() {
  const bridge = renderApp({
    sessions: [fakeSummary(idleSession, idleEvents.length)],
    details: { [session.id]: idleDetail },
  });
  await screen.findByRole("heading", { name: "OAuth device flow" });
  return bridge;
}

function modelTrigger() {
  return screen.getByRole("combobox", { name: "Model for this session" });
}

beforeEach(() => {
  localStorage.clear();
  useSessionStore.getState().reset();
  useWorkspaceStore.getState().reset();
  useContextStore.getState().reset();
  useMenuStore.getState().reset();
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

  it("puts assistant metadata in one ghost footer once the turn is done", async () => {
    renderApp({
      sessions: [fakeSummary({ ...session, awaiting: false }, events.length)],
      details: { [session.id]: { ...detail, awaiting: false } },
    });
    await screen.findByRole("heading", { name: "OAuth device flow" });
    expect(document.querySelector(".turn-head")).toBeNull();
    expect(document.querySelectorAll(".turn-foot")).toHaveLength(1);
    expect(document.querySelector(".turn-user .bubble")).not.toBeNull();
  });

  it("hides the ghost footer while the assistant is still working", async () => {
    await ready();
    expect(document.querySelector(".turn-foot")).toBeNull();
  });

  it("changes the thinking level for the active session", async () => {
    const bridge = await ready();
    const trigger = screen.getByRole("combobox", { name: "Thinking level for this session" });
    await userEvent.click(trigger);
    const listbox = screen.getByRole("listbox", { name: "Thinking level for this session" });
    expect(within(listbox.parentElement as HTMLElement).queryByRole("searchbox")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("option", { name: "High" }));
    await waitFor(() => expect(bridge.invocations).toContain("session_set_thinking"));
    await waitFor(() => expect(trigger).toHaveTextContent("High"));
  });

  it("uses the selected model's own thinking tiers", async () => {
    const mocked = { ...session, provider: "mock", model: "mock-1" };
    const mockedDetail: SessionDetail = {
      ...mocked,
      events,
      thinkingLevel: "off",
    };
    const bridge = renderApp({
      sessions: [fakeSummary(mocked, events.length)],
      details: { [mocked.id]: mockedDetail },
      providerStatuses: [
        fakeProviderStatus({
          id: "mock",
          name: "Offline",
          models: ["mock-1"],
          defaultModel: "mock-1",
          configured: true,
        }),
      ],
      modelThinking: { mock: { "mock-1": ["max"] } },
    });
    await screen.findByRole("heading", { name: "OAuth device flow" });

    // The saved level was off, which beam-free-style models do not support. The composer moves it
    // to the only tier the catalogue announced before the next turn can be sent.
    await waitFor(() =>
      expect(
        bridge.calls.some(
          (call) =>
            call.command === "session_set_thinking" &&
            (call.args as { level?: string }).level === "max",
        ),
      ).toBe(true),
    );
    const trigger = screen.getByRole("combobox", { name: "Thinking level for this session" });
    await waitFor(() => expect(trigger).toHaveTextContent("Max"));
    await userEvent.click(trigger);
    expect(screen.queryByRole("option", { name: "Off" })).not.toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Max" })).toBeInTheDocument();
  });

  it("searches the model combo before switching models", async () => {
    const bridge = await readyIdle();
    await userEvent.click(screen.getByRole("combobox", { name: "Model for this session" }));
    const search = screen.getByRole("searchbox", { name: "Search Model for this session" });
    await userEvent.type(search, "mock");
    expect(screen.getByRole("option", { name: /Mock 1/ })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /Sonnet 4.5/ })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("option", { name: /Mock 1/ }));
    await waitFor(() => expect(bridge.invocations).toContain("session_switch_provider"));
    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "Model for this session" })).toHaveTextContent(
        "Mock 1",
      ),
    );
  });

  it("shows the thinking level and the model name inline, without dropdown chrome", async () => {
    await readyOnAnthropic();

    const thinking = screen.getByRole("combobox", { name: "Thinking level for this session" });
    expect(thinking).toHaveTextContent("Off");
    expect(thinking.querySelector(".combo-trigger-icon svg")).not.toBeNull();
    expect(thinking.querySelector(".combo-chevron")).toBeNull();

    const model = modelTrigger();
    expect(model).toHaveTextContent("Sonnet 4.5");
    expect(model).toHaveAttribute("title", "claude-sonnet-4-5");
    expect(model.querySelector(".combo-trigger-icon svg.provider-glyph")).not.toBeNull();
    expect(model.querySelector(".combo-chevron")).toBeNull();

    expect(document.querySelector(".composer-pickers .thinking-picker")).not.toBeNull();
    expect(document.querySelector(".composer-pickers .model-picker")).not.toBeNull();
  });

  it("groups the model menu by provider and keeps the exact id as the option hint", async () => {
    await readyOnAnthropic();
    await userEvent.click(modelTrigger());

    const listbox = screen.getByRole("listbox", { name: "Model for this session" });
    expect(within(listbox).getByRole("group", { name: "Anthropic" })).toBeInTheDocument();
    expect(within(listbox).getByRole("group", { name: "OpenAI" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Sonnet 4.5 claude-sonnet-4-5" })).toHaveTextContent(
      "claude-sonnet-4-5",
    );
  });

  it("disables an unconfigured provider and opens Providers from the menu footer", async () => {
    await readyOnAnthropic();
    await userEvent.click(modelTrigger());

    const google = screen.getByRole("option", { name: /Gemini 2.5 Pro/ });
    expect(google).toBeDisabled();
    expect(google).toHaveTextContent("Add a key in Settings");

    await userEvent.click(screen.getByRole("button", { name: "Providers" }));
    const tab = await screen.findByRole("tab", { name: "Providers" });
    expect(tab).toHaveAttribute("aria-selected", "true");
  });

  it("searches the model menu across providers", async () => {
    await readyOnAnthropic();
    await userEvent.click(modelTrigger());
    const search = screen.getByRole("searchbox", { name: "Search Model for this session" });
    await userEvent.type(search, "gpt");

    expect(screen.getByRole("option", { name: /GPT-5 Mini/ })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /Sonnet 4.5/ })).not.toBeInTheDocument();
  });

  it("lists remembered models first and prunes ids the catalog no longer has", async () => {
    localStorage.setItem(
      "grove.recentModels",
      JSON.stringify(["claude-opus-4-1", "retired-model"]),
    );
    await readyOnAnthropic();
    await userEvent.click(modelTrigger());

    const listbox = screen.getByRole("listbox", { name: "Model for this session" });
    const recent = within(listbox).getAllByRole("group")[0];
    expect(recent).toHaveAccessibleName("Recent");
    expect(within(recent).getByRole("option", { name: /Opus 4.1/ })).toBeInTheDocument();
    expect(screen.queryByText(/retired-model/)).not.toBeInTheDocument();
    await waitFor(() =>
      expect(JSON.parse(localStorage.getItem("grove.recentModels") ?? "[]")).toEqual([
        "claude-opus-4-1",
      ]),
    );
  });

  it("hot-swaps a same-provider model without relaunching the agent", async () => {
    const bridge = await readyOnAnthropic();
    await userEvent.click(modelTrigger());
    await userEvent.click(screen.getByRole("option", { name: /Opus 4.1/ }));

    await waitFor(() => expect(bridge.invocations).toContain("session_set_model"));
    expect(bridge.invocations).not.toContain("session_switch_provider");
    expect(bridge.calls.find((call) => call.command === "session_set_model")?.args).toEqual({
      id: "session-1",
      model: "claude-opus-4-1",
      thinking: null,
    });
    expect(await screen.findByText("Session now runs on Anthropic · Opus 4.1")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "Model for this session" })).toHaveTextContent(
        "Opus 4.1",
      ),
    );
    expect(JSON.parse(localStorage.getItem("grove.recentModels") ?? "[]")[0]).toBe(
      "claude-opus-4-1",
    );
  });

  it("clamps the thinking level when the new model has a different ladder", async () => {
    const bridge = renderApp({
      providers: PROVIDER_ROSTER,
      providerStatuses: FAKE_PROVIDER_ROSTER,
      sessions: [fakeSummary(anthropicSession, events.length)],
      details: { [anthropicSession.id]: anthropicDetail },
      modelThinking: { anthropic: { "claude-opus-4-1": ["low", "medium"] } },
    });
    await screen.findByRole("heading", { name: "OAuth device flow" });
    await userEvent.click(modelTrigger());
    await userEvent.click(screen.getByRole("option", { name: /Opus 4.1/ }));

    await waitFor(() => expect(bridge.invocations).toContain("session_set_model"));
    expect(bridge.calls.find((call) => call.command === "session_set_model")?.args).toEqual({
      id: "session-1",
      model: "claude-opus-4-1",
      thinking: "low",
    });
    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "Thinking level for this session" })).toHaveTextContent(
        "Low",
      ),
    );
  });

  it("switches provider through the existing path, remembers the choice and toasts the label", async () => {
    const bridge = await readyOnIdleAnthropic();
    await userEvent.click(modelTrigger());
    await userEvent.click(screen.getByRole("option", { name: /GPT-5 Mini/ }));

    await waitFor(() => expect(bridge.invocations).toContain("session_switch_provider"));
    expect(
      bridge.calls.find((call) => call.command === "session_switch_provider")?.args,
    ).toMatchObject({ provider: "openai", model: "gpt-5-mini" });
    expect(await screen.findByText("Session now runs on OpenAI · GPT-5 Mini")).toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem("grove.recentModels") ?? "[]")[0]).toBe("gpt-5-mini");
  });

  it("queues a cross-provider switch while a permission is open and applies it on idle", async () => {
    const bridge = await readyOnAnthropic();
    await userEvent.click(modelTrigger());
    await userEvent.click(screen.getByRole("option", { name: /GPT-5 Mini/ }));

    // The composer says what is coming and why it has not happened yet; the agent is untouched.
    const waiting = await screen.findByTitle(/Waiting for this turn to finish before switching to GPT-5 Mini/);
    expect(waiting).toHaveTextContent("GPT-5 Mini");
    expect(bridge.invocations).not.toContain("session_switch_provider");

    // The user answers the permission and the turn ends; the shell retries at idle.
    act(() => {
      bridge.emitSessionEvent({
        sessionId: session.id,
        payload: {
          at: "2025-01-01T00:00:06.000Z",
          kind: "permission_resolved",
          request_id: "perm-1",
          decision: "approve",
        },
      });
      bridge.emitSessionEvent({
        sessionId: session.id,
        payload: { at: "2025-01-01T00:00:07.000Z", kind: "status", state: "idle" },
      });
    });

    await waitFor(() => expect(bridge.invocations).toContain("session_switch_provider"));
    expect(
      bridge.calls.find((call) => call.command === "session_switch_provider")?.args,
    ).toMatchObject({ id: session.id, provider: "openai", model: "gpt-5-mini" });
    expect(screen.queryByTitle(/Waiting for this turn to finish/)).not.toBeInTheDocument();
    expect(await screen.findByText("Session now runs on OpenAI · GPT-5 Mini")).toBeInTheDocument();
  });

  it("drops a queued switch when the session exits before it can be applied", async () => {
    const bridge = await readyOnAnthropic();
    await userEvent.click(modelTrigger());
    await userEvent.click(screen.getByRole("option", { name: /GPT-5 Mini/ }));
    expect(
      await screen.findByTitle(/Waiting for this turn to finish before switching to GPT-5 Mini/),
    ).toBeInTheDocument();

    act(() => {
      bridge.emitSessionEvent({
        sessionId: session.id,
        payload: { at: "2025-01-01T00:00:06.000Z", kind: "status", state: "exited" },
      });
    });

    await waitFor(() =>
      expect(screen.queryByTitle(/Waiting for this turn to finish/)).not.toBeInTheDocument(),
    );
    expect(bridge.invocations).not.toContain("session_switch_provider");
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
    await userEvent.click(screen.getByRole("button", { name: "New chat" }));
    const title = screen.getByLabelText("What should this session work on?");
    await userEvent.type(title, "Add the session tabs");
    await userEvent.click(screen.getByRole("button", { name: "Start session" }));
    await screen.findByRole("heading", { name: "Add the session tabs" });
    expect(bridge.invocations).toContain("session_create");
    expect(bridge.calls.find((call) => call.command === "session_create")?.args).toMatchObject({
      req: { config: { thinkingLevel: "off" } },
    });
  });

  it("auto-resumes a stopped session when you send", async () => {
    const stopped = { ...session, status: "exited" as const, awaiting: false };
    const bridge = renderApp({
      sessions: [fakeSummary(stopped, events.length)],
      details: { [session.id]: { ...detail, status: "exited", awaiting: false } },
    });
    await screen.findByRole("heading", { name: "OAuth device flow" });

    const composer = screen.getByLabelText("Message the active session");
    expect(composer).toBeEnabled();
    await userEvent.type(composer, "Try the OpenCode models");
    expect(composer).toHaveValue("Try the OpenCode models");
    expect(screen.queryByRole("button", { name: "Resume session" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Send message" }));
    await waitFor(() => expect(bridge.invocations).toContain("session_resume"));
    await waitFor(() => expect(bridge.invocations).toContain("session_send"));
  });

  it("drives the custom window controls", async () => {
    const bridge = await ready();

    await userEvent.click(screen.getByRole("button", { name: "Minimize window" }));
    expect(bridge.invocations).toContain("app_window_minimize");

    await userEvent.click(screen.getByRole("button", { name: "Maximize window" }));
    await waitFor(() => expect(bridge.invocations).toContain("app_window_toggle_maximize"));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Restore window" })).toBeInTheDocument(),
    );

    await userEvent.click(screen.getByRole("button", { name: "Close window" }));
    expect(bridge.invocations).toContain("app_window_close");
  });

  it("sends immediately from a ready session instead of queueing", async () => {
    const ready = { ...session, status: "ready" as const, awaiting: false };
    const bridge = renderApp({
      sessions: [fakeSummary(ready, events.length)],
      details: { [session.id]: { ...detail, status: "ready", awaiting: false } },
    });
    await screen.findByRole("heading", { name: "OAuth device flow" });

    const composer = screen.getByLabelText("Message the active session");
    await userEvent.type(composer, "Use the OpenCode models");
    await userEvent.click(screen.getByRole("button", { name: "Send message" }));
    await waitFor(() => expect(bridge.invocations).toContain("session_send"));
  });

  it("answers an inline question and renders the resolved summary", async () => {
    const questionEvents: SessionEvent[] = [
      {
        at: "2025-01-01T00:00:01.000Z",
        kind: "question_request",
        request_id: "question-1",
        questions: [
          {
            id: "scope",
            title: "Which scope?",
            hint: "This decides the diff size.",
            multi: false,
            other: true,
            options: [
              { label: "Session module only", description: "Keep the diff small." },
              { label: "Whole workspace", description: "Touch every caller." },
            ],
          },
        ],
      },
    ];
    const waiting = { ...session, awaiting: true };
    const bridge = renderApp({
      sessions: [fakeSummary(waiting, questionEvents.length)],
      details: {
        [session.id]: { ...detail, events: questionEvents, awaiting: true },
      },
    });
    await screen.findByRole("heading", { name: "OAuth device flow" });

    await userEvent.click(screen.getByRole("radio", { name: /Session module only/ }));
    await userEvent.click(screen.getByRole("button", { name: "Send answer" }));
    await waitFor(() => expect(bridge.invocations).toContain("session_respond_question"));
    await screen.findByRole("group", { name: "Answered questions" });
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
    const address = await within(dialog).findByLabelText("Folder path");
    await waitFor(() => expect(address).toHaveValue("C:/Users/demo"));
    await userEvent.click(within(dialog).getByRole("button", { name: "Select folder" }));
    await userEvent.click(within(dialog).getByRole("button", { name: "Add project" }));

    await waitFor(() =>
      expect(bridge.calls.some((call) => call.command === "project_add")).toBe(true),
    );
    expect(await screen.findByText("demo")).toBeInTheDocument();
  });

  it("navigates the folder picker to a path pasted from another drive", async () => {
    const { bridge, dialog, address } = await openPicker();

    await userEvent.clear(address);
    await userEvent.type(address, "D:\\Dev\\grove");
    await userEvent.keyboard("{Enter}");

    await waitFor(() => expect(address).toHaveValue("D:/Dev/grove"));
    expect(await within(dialog).findByRole("option", { name: "docs" })).toBeInTheDocument();
    expect(
      bridge.calls.some(
        (call) =>
          call.command === "fs_list_dirs" &&
          (call.args as { path?: string }).path === "D:/Dev/grove",
      ),
    ).toBe(true);

    // A forward-slash path navigates the same way, and Escape puts the address back.
    await userEvent.clear(address);
    await userEvent.type(address, "D:/Dev{Enter}");
    await waitFor(() => expect(address).toHaveValue("D:/Dev"));
    await userEvent.clear(address);
    await userEvent.type(address, "not a folder{Escape}");
    expect(address).toHaveValue("D:/Dev");
  });

  it("keeps the current listing when a pasted path does not exist", async () => {
    const { dialog, address } = await openPicker();

    await userEvent.clear(address);
    await userEvent.type(address, "Q:/missing{Enter}");

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(/does not exist/);
    expect(within(dialog).getByRole("option", { name: "code" })).toBeInTheDocument();
    expect(address).toHaveValue("Q:/missing");
  });

  it("moves through folders with the keyboard and goes back up", async () => {
    const { dialog, address } = await openPicker();
    const list = within(dialog).getByRole("listbox", { name: "Folders" });

    list.focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(within(dialog).getByRole("option", { name: "code" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await userEvent.keyboard("{ArrowDown}{ArrowUp}{Enter}");
    await waitFor(() => expect(address).toHaveValue("C:/Users/ada/code"));

    await userEvent.keyboard("{Backspace}");
    await waitFor(() => expect(address).toHaveValue("C:/Users/ada"));
    expect(within(dialog).getByRole("option", { name: "notes" })).toBeInTheDocument();
  });

  it("enters a folder by clicking its row", async () => {
    const { dialog, address } = await openPicker();

    await userEvent.click(within(dialog).getByRole("option", { name: "code" }));
    await waitFor(() => expect(address).toHaveValue("C:/Users/ada/code"));
    expect(within(dialog).getByRole("option", { name: "grove" })).toBeInTheDocument();
    expect(within(dialog).queryByRole("option", { name: "notes" })).not.toBeInTheDocument();

    await userEvent.click(within(dialog).getByRole("option", { name: "grove" }));
    await waitFor(() => expect(address).toHaveValue("C:/Users/ada/code/grove"));

    // The breadcrumb is a way back up without retyping the path.
    await userEvent.click(within(dialog).getByRole("button", { name: "code" }));
    await waitFor(() => expect(address).toHaveValue("C:/Users/ada/code"));
  });

  it("returns to the project form after Select folder and seeds detection", async () => {
    const { bridge, dialog, address } = await openPicker();

    await userEvent.clear(address);
    await userEvent.type(address, "D:/Dev/grove{Enter}");
    await within(dialog).findByRole("option", { name: "docs" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Select folder" }));

    expect(await within(dialog).findByText("D:/Dev/grove")).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Name")).toHaveValue("grove");
    expect(within(dialog).getByLabelText("Default branch")).toHaveValue("main");
    expect(within(dialog).getByLabelText(/Remote/)).toHaveValue("acme/grove");
    expect(
      bridge.calls.some(
        (call) =>
          call.command === "project_detect" &&
          (call.args as { path?: string }).path === "D:/Dev/grove",
      ),
    ).toBe(true);
  });

  it("creates a folder inline and enters it", async () => {
    const { bridge, dialog, address } = await openPicker();

    await userEvent.click(within(dialog).getByRole("button", { name: "New folder" }));
    const name = within(dialog).getByLabelText("New folder name");
    await userEvent.type(name, "new-project{Enter}");

    await waitFor(() => expect(address).toHaveValue("C:/Users/ada/new-project"));
    const call = bridge.calls.find((entry) => entry.command === "fs_create_dir");
    expect(call?.args).toEqual({ parent: "C:/Users/ada", name: "new-project" });
  });

  it("shows an invalid new-folder name inline and stays put", async () => {
    const { bridge, dialog, address } = await openPicker();

    await userEvent.click(within(dialog).getByRole("button", { name: "New folder" }));
    await userEvent.type(within(dialog).getByLabelText("New folder name"), "bad/name{Enter}");

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(/single path segment/);
    expect(address).toHaveValue("C:/Users/ada");
    expect(bridge.invocations).toContain("fs_create_dir");
  });

  it("renders the drive chips from the contract fixture and jumps by chip", async () => {
    const { dialog, address } = await openPicker();

    expect(within(dialog).getByRole("button", { name: "Home folder" })).toHaveAttribute(
      "aria-current",
      "true",
    );
    expect(within(dialog).getByRole("button", { name: "C:" })).toBeInTheDocument();

    await userEvent.click(within(dialog).getByRole("button", { name: "D:" }));
    await waitFor(() => expect(address).toHaveValue("D:/"));
    expect(await within(dialog).findByRole("option", { name: "Dev" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "D:" })).toHaveAttribute(
      "aria-current",
      "true",
    );
    expect(within(dialog).getByRole("button", { name: "C:" })).not.toHaveAttribute(
      "aria-current",
    );
  });

  it("opens a subagent's forwarded transcript from the tree", async () => {
    const subagentEvents: SessionEvent[] = [
      {
        at: "2025-01-01T00:00:05.000Z",
        kind: "tool_call",
        call_id: "call-sub",
        name: "subagent",
        title: "subagent · audit the store",
      },
      {
        at: "2025-01-01T00:00:06.000Z",
        kind: "tool_call",
        call_id: "sub-call-sub-read",
        name: "read_file",
        title: "subagent · read_file crates/store.rs",
      },
      {
        at: "2025-01-01T00:00:06.500Z",
        kind: "tool_result",
        call_id: "sub-call-sub-read",
        status: "ok",
        summary: "the store",
      },
      {
        at: "2025-01-01T00:00:07.000Z",
        kind: "tool_result",
        call_id: "call-sub",
        status: "ok",
        summary: "worker done",
      },
    ];
    renderApp({
      sessions: [
        {
          ...fakeSummary({ ...session, awaiting: true }, events.length + subagentEvents.length),
          subagents: [
            {
              callId: "call-sub",
              description: "audit the store",
              status: "ok",
              summary: "worker done",
              startedAt: "2025-01-01T00:00:05.000Z",
              finishedAt: "2025-01-01T00:00:07.000Z",
            },
          ],
        },
      ],
      details: {
        [session.id]: { ...detail, events: [...events, ...subagentEvents] },
      },
    });
    await screen.findByRole("heading", { name: "OAuth device flow" });

    const nav = document.getElementById("panel-nav") as HTMLElement;
    await userEvent.click(within(nav).getByRole("button", { name: /audit the store/ }));
    expect(await screen.findByRole("heading", { name: "audit the store" })).toBeInTheDocument();
    expect(screen.getByText("read_file crates/store.rs")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Session" }));
    expect(await screen.findByRole("heading", { name: "OAuth device flow" })).toBeInTheDocument();
  });
});

describe("context menu", () => {
  const clipboard = {
    writeText: vi.fn(async (_text: string) => {}),
    readText: vi.fn(async () => ""),
  };

  beforeEach(() => {
    clipboard.writeText.mockClear();
    clipboard.readText.mockClear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    Reflect.deleteProperty(navigator, "clipboard");
  });

  function useClipboard() {
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: clipboard });
  }

  function sessionRow(): HTMLElement {
    return screen.getByRole("button", { name: /OAuth device flow/ });
  }

  it("opens the session menu on right-click and runs its actions", async () => {
    useClipboard();
    const bridge = await ready();

    const dispatched = fireEvent.contextMenu(sessionRow(), { clientX: 120, clientY: 160 });
    expect(dispatched).toBe(false);
    const menu = await screen.findByRole("menu");
    expect(
      within(menu).getByRole("menuitem", { name: "Copy working directory" }),
    ).toBeInTheDocument();

    await userEvent.click(within(menu).getByRole("menuitem", { name: "Copy working directory" }));
    await waitFor(() => expect(clipboard.writeText).toHaveBeenCalledWith(session.cwd));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    fireEvent.contextMenu(sessionRow(), { clientX: 120, clientY: 160 });
    await userEvent.click(
      within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Reveal in Explorer" }),
    );
    await waitFor(() =>
      expect(
        bridge.calls.some(
          (call) =>
            call.command === "app_reveal_path" &&
            (call.args as { path: string }).path === session.cwd,
        ),
      ).toBe(true),
    );
  });

  it("moves with the arrow keys, Home and End, and Escape closes back to the row", async () => {
    await ready();
    const row = sessionRow();
    fireEvent.contextMenu(row, { clientX: 100, clientY: 100 });
    const menu = await screen.findByRole("menu");
    const items = within(menu).getAllByRole("menuitem");

    await waitFor(() => expect(items[0]).toHaveFocus());
    await userEvent.keyboard("{ArrowDown}");
    expect(items[1]).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(items[0]).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(items[items.length - 1]).toHaveFocus();
    await userEvent.keyboard("{ArrowUp}");
    expect(items[items.length - 2]).toHaveFocus();

    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());
    expect(row).toHaveFocus();

    fireEvent.contextMenu(row, { clientX: 100, clientY: 100 });
    await screen.findByRole("menu");
    await userEvent.keyboard("{Tab}");
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());
  });

  it("activates the focused item with Enter", async () => {
    const bridge = await ready();
    fireEvent.contextMenu(sessionRow(), { clientX: 100, clientY: 100 });
    const menu = await screen.findByRole("menu");
    await waitFor(() => expect(within(menu).getAllByRole("menuitem")[0]).toHaveFocus());

    // Focus the item by name: the menu gains items over time, so its position is not the contract.
    within(menu).getByRole("menuitem", { name: "Reveal in Explorer" }).focus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() =>
      expect(
        bridge.calls.some(
          (call) =>
            call.command === "app_reveal_path" &&
            (call.args as { path: string }).path === session.cwd,
        ),
      ).toBe(true),
    );
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("closes on an outside pointer press and returns focus to the row", async () => {
    await ready();
    const row = sessionRow();
    fireEvent.contextMenu(row, { clientX: 100, clientY: 100 });
    await screen.findByRole("menu");

    fireEvent.pointerDown(document.body);
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());
    expect(row).toHaveFocus();
  });

  it("opens from the hover-only, keyboard-focusable session actions button", async () => {
    await ready();
    const actions = screen.getByRole("button", { name: "Session actions" });
    actions.focus();
    expect(actions).toHaveFocus();

    await userEvent.click(actions);
    expect(await screen.findByRole("menu")).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Copy working directory" })).toBeInTheDocument();
  });

  it("offers the text edit actions and omits Paste when clipboard read is denied", async () => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: vi.fn(async () => {}),
        readText: vi.fn(async () => {
          throw new Error("denied");
        }),
      },
    });
    await ready();
    const composer = screen.getByLabelText("Message the active session");

    fireEvent.contextMenu(composer, { clientX: 40, clientY: 40 });
    const menu = await screen.findByRole("menu");
    for (const label of ["Undo", "Redo", "Cut", "Copy", "Select all"]) {
      expect(within(menu).getByRole("menuitem", { name: label })).toBeInTheDocument();
    }
    expect(within(menu).queryByRole("menuitem", { name: "Paste" })).not.toBeInTheDocument();
  });

  it("copies selected text when execCommand is unavailable", async () => {
    useClipboard();
    await ready();
    const filter = screen.getByLabelText(/Filter projects/);
    await userEvent.type(filter, "oauth");
    (filter as HTMLInputElement).setSelectionRange(0, 5);

    fireEvent.contextMenu(filter, { clientX: 40, clientY: 40 });
    await userEvent.click(
      within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Copy" }),
    );
    await waitFor(() => expect(clipboard.writeText).toHaveBeenCalledWith("oauth"));
  });

  it("pastes clipboard text into the composer when execCommand is unavailable", async () => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: vi.fn(async () => {}),
        readText: vi.fn(async () => "pasted text"),
      },
    });
    await ready();
    const composer = screen.getByLabelText("Message the active session");

    fireEvent.contextMenu(composer, { clientX: 40, clientY: 40 });
    await userEvent.click(
      within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Paste" }),
    );

    await waitFor(() =>
      expect(useSessionStore.getState().drafts[session.id]).toBe("pasted text"),
    );
    expect(composer).toHaveValue("pasted text");
  });

  it("opens the project menu and removes only the project row", async () => {
    const bridge = await ready();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const nav = document.getElementById("panel-nav") as HTMLElement;

    fireEvent.contextMenu(within(nav).getByText("grove"), { clientX: 40, clientY: 60 });
    const menu = await screen.findByRole("menu");
    for (const label of ["New session", "Copy path", "Reveal in Explorer", "Refresh"]) {
      expect(within(menu).getByRole("menuitem", { name: label })).toBeInTheDocument();
    }

    await userEvent.click(within(menu).getByRole("menuitem", { name: "Remove project" }));
    await waitFor(() => expect(confirm).toHaveBeenCalled());
    await waitFor(() =>
      expect(
        bridge.calls.some(
          (call) =>
            call.command === "project_remove" &&
            (call.args as { id: string }).id === project.id,
        ),
      ).toBe(true),
    );
    await waitFor(() => expect(within(nav).queryByText("grove")).not.toBeInTheDocument());
    expect(within(nav).getByText("Chats")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /OAuth device flow/ })).toBeInTheDocument();
  });

  it("opens the worktree menu and starts a session there", async () => {
    await ready();
    const nav = document.getElementById("panel-nav") as HTMLElement;

    fireEvent.contextMenu(within(nav).getByText("feat/ui"), { clientX: 40, clientY: 60 });
    const menu = await screen.findByRole("menu");
    for (const label of ["New session", "Copy path", "Reveal in Explorer", "Refresh"]) {
      expect(within(menu).getByRole("menuitem", { name: label })).toBeInTheDocument();
    }

    await userEvent.click(within(menu).getByRole("menuitem", { name: "New session" }));
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(screen.getByText(/joins feat\/ui/)).toBeInTheDocument();
  });

  it("opens the background menu on empty tree space", async () => {
    await ready();
    const nav = document.getElementById("panel-nav") as HTMLElement;
    const scroller = nav.querySelector(".nav-scroll") as HTMLElement;

    fireEvent.contextMenu(scroller, { clientX: 20, clientY: 400 });
    const menu = await screen.findByRole("menu");
    for (const label of ["New chat", "Add project", "Refresh"]) {
      expect(within(menu).getByRole("menuitem", { name: label })).toBeInTheDocument();
    }

    await userEvent.click(within(menu).getByRole("menuitem", { name: "New chat" }));
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
  });

  it("portals the menu into the topmost open dialog", async () => {
    await ready();
    await userEvent.click(screen.getByRole("button", { name: /Add project/ }));
    const addDialog = screen.getByRole("dialog");

    fireEvent.contextMenu(addDialog, { clientX: 20, clientY: 20 });
    expect((await screen.findByRole("menu")).closest("dialog")).toBe(addDialog);

    act(() => useUiStore.getState().openSettings("general"));
    await waitFor(() => expect(document.querySelectorAll("dialog[open]")).toHaveLength(2));
    const dialogs = document.querySelectorAll<HTMLDialogElement>("dialog[open]");
    const topmost = dialogs[dialogs.length - 1];

    fireEvent.contextMenu(topmost, { clientX: 20, clientY: 20 });
    const menu = await screen.findByRole("menu");
    expect(menu.closest("dialog")).toBe(topmost);
  });

  it("keeps the menu inside the viewport when opened near an edge", async () => {
    await ready();
    const box = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 0,
      y: 0,
      top: 0,
      left: 0,
      right: 240,
      bottom: 160,
      width: 240,
      height: 160,
      toJSON: () => ({}),
    });

    fireEvent.contextMenu(sessionRow(), { clientX: 1000, clientY: 760 });
    const menu = await screen.findByRole("menu");
    await waitFor(() => expect(menu.style.left).toBe("760px"));
    expect(menu.style.top).toBe("600px");
    box.mockRestore();
  });

  it("closes on scroll, resize and window blur", async () => {
    await ready();
    for (const close of [
      () => window.dispatchEvent(new Event("scroll")),
      () => window.dispatchEvent(new Event("resize")),
      () => window.dispatchEvent(new Event("blur")),
    ]) {
      fireEvent.contextMenu(sessionRow(), { clientX: 60, clientY: 60 });
      await screen.findByRole("menu");
      act(close);
      await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());
    }
  });

  it("opens the message menu and copies the message", async () => {
    useClipboard();
    await ready();

    fireEvent.contextMenu(screen.getByText("Wire up the GitHub OAuth device flow."), {
      clientX: 50,
      clientY: 50,
    });
    const menu = await screen.findByRole("menu");
    expect(within(menu).getByRole("menuitem", { name: "Copy" })).toBeInTheDocument();
    expect(within(menu).getByRole("menuitem", { name: "Copy as Markdown" })).toBeInTheDocument();
    expect(within(menu).queryByRole("menuitem", { name: "Copy code" })).not.toBeInTheDocument();

    await userEvent.click(within(menu).getByRole("menuitem", { name: "Copy" }));
    await waitFor(() =>
      expect(clipboard.writeText).toHaveBeenCalledWith("Wire up the GitHub OAuth device flow."),
    );
  });

  it("offers Copy code when the message contains a code block", async () => {
    useClipboard();
    const coded: SessionEvent[] = [
      {
        at: "2025-01-01T00:00:02.000Z",
        kind: "message",
        role: "assistant",
        blocks: [
          { type: "text", text: "Here it is." },
          { type: "code", language: "rust", text: "fn main() {}" },
        ],
      },
    ];
    renderApp({
      sessions: [fakeSummary({ ...session, awaiting: false }, coded.length)],
      details: { [session.id]: { ...detail, events: coded, awaiting: false } },
    });
    await screen.findByRole("heading", { name: "OAuth device flow" });

    fireEvent.contextMenu(screen.getByText("Here it is."), { clientX: 50, clientY: 50 });
    const menu = await screen.findByRole("menu");
    await userEvent.click(
      within(menu).getByRole("menuitem", { name: "Copy as Markdown" }),
    );
    await waitFor(() =>
      expect(clipboard.writeText).toHaveBeenCalledWith(
        "Here it is.\n\n```rust\nfn main() {}\n```",
      ),
    );

    fireEvent.contextMenu(screen.getByText("Here it is."), { clientX: 50, clientY: 50 });
    await userEvent.click(
      within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Copy code" }),
    );
    await waitFor(() => expect(clipboard.writeText).toHaveBeenCalledWith("fn main() {}"));
  });

  it("opens the tool row menu and reveals the resolved path", async () => {
    const bridge = await ready();

    fireEvent.contextMenu(screen.getByText("apps/desktop/src/session.rs"), {
      clientX: 50,
      clientY: 50,
    });
    const menu = await screen.findByRole("menu");
    expect(within(menu).getByRole("menuitem", { name: "Copy path" })).toBeInTheDocument();

    await userEvent.click(within(menu).getByRole("menuitem", { name: "Reveal in Explorer" }));
    await waitFor(() =>
      expect(
        bridge.calls.some(
          (call) =>
            call.command === "app_reveal_path" &&
            (call.args as { path: string }).path ===
              "C:/code/grove/apps/desktop/src/session.rs",
        ),
      ).toBe(true),
    );
  });
});

describe("archive and delete", () => {
  const archivedAt = "2025-01-02T00:00:00.000Z";
  const archivedSession = {
    ...session,
    id: "session-2",
    title: "Old spike",
    status: "exited" as const,
    awaiting: false,
    archivedAt,
  };
  const archivedDetail: SessionDetail = { ...detail, ...archivedSession, events: [] };

  function archiveSeed(): FakeBridgeSeed {
    return {
      sessions: [
        fakeSummary({ ...session, awaiting: true }, events.length),
        fakeSummary(archivedSession, 0),
      ],
      details: { [session.id]: detail, [archivedSession.id]: archivedDetail },
    };
  }

  function nav(): HTMLElement {
    return document.getElementById("panel-nav") as HTMLElement;
  }

  function treeRow(title: string): HTMLElement {
    return within(nav()).getByRole("button", { name: new RegExp(title) });
  }

  async function openArchived(): Promise<void> {
    await userEvent.click(treeRow("Old spike"));
    await screen.findByRole("heading", { name: "Old spike" });
  }

  it("lists archived sessions in a counted Archive section, apart from the active tree", async () => {
    renderApp(archiveSeed());
    await screen.findByRole("heading", { name: "OAuth device flow" });

    expect(within(nav()).getByRole("button", { name: /Archive/ })).toBeInTheDocument();
    expect(within(nav()).getByText("Old spike")).toBeInTheDocument();
    expect(nav().querySelector("#kids-archive .s-suffix")).toHaveTextContent("grove");
    expect(within(nav()).getByText("1 session · 1 awaiting you")).toBeInTheDocument();

    await userEvent.click(within(nav()).getByRole("button", { name: "Toggle archive" }));
    expect(document.getElementById("kids-archive")).toHaveAttribute("hidden");
  });

  it("shows an empty Archive section when nothing is archived", async () => {
    await ready();
    expect(within(nav()).getByText("No archived sessions")).toBeInTheDocument();
  });

  it("searches the Archive section and keeps its header when a row matches", async () => {
    renderApp(archiveSeed());
    await screen.findByRole("heading", { name: "OAuth device flow" });

    await userEvent.type(screen.getByLabelText(/Filter projects/), "old");

    expect(within(nav()).getByText("Old spike")).toBeInTheDocument();
    expect(within(nav()).getByRole("button", { name: /Archive/ })).toBeInTheDocument();
    expect(within(nav()).queryByText("OAuth device flow")).not.toBeInTheDocument();
    expect(within(nav()).queryByText(/Nothing matches/)).not.toBeInTheDocument();
  });

  it("opens an archived session with a pill and restores it from the header", async () => {
    const bridge = renderApp(archiveSeed());
    await screen.findByRole("heading", { name: "OAuth device flow" });
    await openArchived();

    expect(screen.getByText("Archived")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Restore session" }));

    await waitFor(() => expect(bridge.invocations).toContain("session_restore"));
    await waitFor(() => expect(screen.queryByText("Archived")).not.toBeInTheDocument());
    expect(await screen.findByText("Session restored")).toBeInTheDocument();
  });

  it("restores an archived session before sending and keeps the composer enabled", async () => {
    const bridge = renderApp(archiveSeed());
    await screen.findByRole("heading", { name: "OAuth device flow" });
    await openArchived();

    const composer = screen.getByLabelText("Message the active session");
    expect(composer).toBeEnabled();
    await userEvent.type(composer, "One more thing");
    await userEvent.click(screen.getByRole("button", { name: "Send message" }));

    await waitFor(() => expect(bridge.invocations).toContain("session_send"));
    const restoreAt = bridge.invocations.indexOf("session_restore");
    const sendAt = bridge.invocations.indexOf("session_send");
    expect(restoreAt).toBeGreaterThanOrEqual(0);
    expect(restoreAt).toBeLessThan(sendAt);
    expect(await screen.findByText("Session restored")).toBeInTheDocument();
    expect(screen.queryByText("Archived")).not.toBeInTheDocument();
  });

  it("archives a running session only after confirmation, then moves it to Archive", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    const bridge = await ready();

    fireEvent.contextMenu(treeRow("OAuth device flow"), { clientX: 120, clientY: 160 });
    await userEvent.click(
      within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Archive" }),
    );
    expect(confirm).toHaveBeenCalledWith(expect.stringContaining("OAuth device flow"));
    expect(bridge.invocations).not.toContain("session_archive");

    confirm.mockReturnValue(true);
    fireEvent.contextMenu(treeRow("OAuth device flow"), { clientX: 120, clientY: 160 });
    await userEvent.click(
      within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Archive" }),
    );

    await waitFor(() => expect(bridge.invocations).toContain("session_archive"));
    expect(bridge.calls.find((call) => call.command === "session_archive")?.args).toEqual({
      id: session.id,
    });
    expect(await screen.findByText("Archived")).toBeInTheDocument();
    expect(within(nav()).getByText("0 sessions · 0 awaiting you")).toBeInTheDocument();
  });

  it("offers permanent delete only on an archived row, after a confirmation that names it", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const bridge = renderApp(archiveSeed());
    await screen.findByRole("heading", { name: "OAuth device flow" });

    fireEvent.contextMenu(treeRow("OAuth device flow"), { clientX: 120, clientY: 160 });
    const activeMenu = await screen.findByRole("menu");
    expect(
      within(activeMenu).queryByRole("menuitem", { name: "Delete permanently" }),
    ).not.toBeInTheDocument();
    expect(within(activeMenu).queryByRole("menuitem", { name: "Restore" })).not.toBeInTheDocument();
    await userEvent.keyboard("{Escape}");

    await openArchived();
    fireEvent.contextMenu(treeRow("Old spike"), { clientX: 120, clientY: 160 });
    await userEvent.click(
      within(await screen.findByRole("menu")).getByRole("menuitem", {
        name: "Delete permanently",
      }),
    );

    await waitFor(() => expect(bridge.invocations).toContain("session_delete"));
    expect(confirm).toHaveBeenCalledWith(expect.stringContaining("Old spike"));
    expect(bridge.calls.find((call) => call.command === "session_delete")?.args).toEqual({
      id: archivedSession.id,
    });
    await waitFor(() => expect(within(nav()).queryByText("Old spike")).not.toBeInTheDocument());
    expect(screen.queryByText("Archived")).not.toBeInTheDocument();
    await screen.findByRole("heading", { name: "OAuth device flow" });
  });

  it("ignores archived sessions for restore-on-launch and the awaiting badge", async () => {
    localStorage.setItem("grove.lastSession", archivedSession.id);
    renderApp({
      sessions: [
        fakeSummary({ ...session, awaiting: false }, events.length),
        fakeSummary({ ...archivedSession, awaiting: true }, 0),
      ],
      details: {
        [session.id]: { ...detail, awaiting: false },
        [archivedSession.id]: { ...archivedDetail, awaiting: true },
      },
    });

    await screen.findByRole("heading", { name: "OAuth device flow" });
    expect(screen.queryByText("Archived")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /awaiting you - open that session/ }),
    ).not.toBeInTheDocument();
    expect(within(nav()).getByText("1 session · 0 awaiting you")).toBeInTheDocument();
  });
});

describe("session rename", () => {
  const nav = () => document.getElementById("panel-nav") as HTMLElement;

  function row(name: RegExp): HTMLElement {
    return within(nav()).getByRole("button", { name });
  }

  /** Right-click the active session, choose Rename, and return the inline field. */
  async function startRename() {
    const bridge = await ready();
    fireEvent.contextMenu(row(/OAuth device flow/), { clientX: 120, clientY: 160 });
    const menu = await screen.findByRole("menu");
    await userEvent.click(within(menu).getByRole("menuitem", { name: "Rename" }));
    const input = (await screen.findByRole("textbox", {
      name: "Session title",
    })) as HTMLInputElement;
    return { bridge, input };
  }

  function renames(bridge: Awaited<ReturnType<typeof ready>>) {
    return bridge.calls.filter((call) => call.command === "session_rename");
  }

  it("renames inline from the menu and shows the stored title on the row and header", async () => {
    const { bridge, input } = await startRename();

    expect(input).toHaveValue("OAuth device flow");
    expect(input).toHaveFocus();
    expect(input.selectionStart).toBe(0);
    expect(input.selectionEnd).toBe("OAuth device flow".length);

    await userEvent.clear(input);
    await userEvent.type(input, "  Release notes  ");
    await userEvent.keyboard("{Enter}");

    await waitFor(() => expect(renames(bridge)).toHaveLength(1));
    expect(renames(bridge)[0].args).toEqual({ id: session.id, title: "Release notes" });
    await waitFor(() =>
      expect(row(/Release notes/)).toBeInTheDocument(),
      { timeout: 2000 },
    );
    expect(screen.getByRole("heading", { name: "Release notes" })).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Session title" })).not.toBeInTheDocument();
  });

  it("commits the edit on blur", async () => {
    const { bridge, input } = await startRename();

    await userEvent.clear(input);
    await userEvent.type(input, "Blurred title");
    await userEvent.click(screen.getByRole("searchbox", { name: /Filter projects/ }));

    await waitFor(() => expect(renames(bridge)).toHaveLength(1));
    expect(renames(bridge)[0].args).toEqual({ id: session.id, title: "Blurred title" });
    await waitFor(() => expect(row(/Blurred title/)).toBeInTheDocument());
  });

  it("cancels on Escape without calling the core", async () => {
    const { bridge, input } = await startRename();

    await userEvent.clear(input);
    await userEvent.type(input, "Discard me");
    await userEvent.keyboard("{Escape}");

    await waitFor(() =>
      expect(screen.queryByRole("textbox", { name: "Session title" })).not.toBeInTheDocument(),
    );
    expect(renames(bridge)).toHaveLength(0);
    expect(row(/OAuth device flow/)).toBeInTheDocument();
  });

  it("cancels when the title is emptied", async () => {
    const { bridge, input } = await startRename();

    await userEvent.clear(input);
    await userEvent.keyboard("{Enter}");

    await waitFor(() =>
      expect(screen.queryByRole("textbox", { name: "Session title" })).not.toBeInTheDocument(),
    );
    expect(renames(bridge)).toHaveLength(0);
    expect(row(/OAuth device flow/)).toBeInTheDocument();
  });

  it("keeps the field open with an inline error when the title is too long", async () => {
    const { bridge, input } = await startRename();

    await userEvent.clear(input);
    fireEvent.change(input, { target: { value: "x".repeat(81) } });
    await userEvent.keyboard("{Enter}");

    expect(await screen.findByRole("alert")).toHaveTextContent("80");
    expect(screen.getByRole("textbox", { name: "Session title" })).toBeInTheDocument();
    expect(renames(bridge)).toHaveLength(0);
  });

  it("does not open the session while its row is being edited", async () => {
    const { bridge, input } = await startRename();
    const opens = () => bridge.calls.filter((call) => call.command === "session_get").length;
    const before = opens();

    expect(within(nav()).queryByRole("button", { name: /OAuth device flow/ })).not.toBeInTheDocument();
    await userEvent.click(input);

    expect(opens()).toBe(before);
    expect(screen.getByRole("textbox", { name: "Session title" })).toHaveFocus();
  });
});
