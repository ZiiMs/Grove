import type { HarnessEvent, SessionDetail, SessionSummary } from "@grove/contract";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { BridgeProvider } from "../../ipc/context";
import { useSessionStore } from "../../state/sessionStore";
import {
  createFakeBridge,
  fakeSession,
  fakeSummary,
  MOCK_CAPABILITIES,
  type FakeBridge,
} from "../../test-utils/fakeBridge";
import { SessionsPanel } from ".";

const AT = "2025-01-01T00:00:01.000Z";

function detail(status: SessionDetail["status"] = "idle", events: HarnessEvent[] = []): SessionDetail {
  return { ...fakeSession({ status }), events };
}

function renderPanel(bridge: FakeBridge) {
  render(
    <BridgeProvider bridge={bridge}>
      <SessionsPanel />
    </BridgeProvider>,
  );
}

beforeEach(() => {
  useSessionStore.getState().reset();
});

describe("SessionsPanel", () => {
  it("lists sessions from the core and opens one", async () => {
    const row = detail("idle", [
      { at: AT, kind: "message", role: "user", blocks: [{ type: "text", text: "hello" }] },
    ]);
    const summary: SessionSummary = fakeSummary(row, row.events.length);
    const bridge = createFakeBridge({}, { sessions: [summary], details: { [row.id]: row } });
    renderPanel(bridge);

    const item = await screen.findByTestId("session-item-session-1");
    expect(within(item).getByText("mock session")).toBeInTheDocument();

    await userEvent.click(item);

    expect(await screen.findByTestId("transcript-message")).toHaveTextContent("hello");
    expect(bridge.invocations).toContain("session_get");
  });

  it("creates a session with a harness picked from the registered list", async () => {
    const bridge = createFakeBridge();
    renderPanel(bridge);

    expect(await screen.findByTestId("harness-picker")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByTestId("session-create")).toBeEnabled());

    await userEvent.type(screen.getByLabelText("Project directory"), "C:/work/grove");
    await userEvent.click(screen.getByTestId("session-create"));

    await waitFor(() => {
      const call = bridge.calls.find((entry) => entry.command === "session_create");
      expect(call?.args).toEqual({ req: { harness: "mock", cwd: "C:/work/grove" } });
    });
    expect(await screen.findByTestId("session-item-session-1")).toBeInTheDocument();
  });

  it("forwards an adapter config on create when one is set", async () => {
    const bridge = createFakeBridge();
    renderPanel(bridge);
    await screen.findByTestId("harness-picker");

    await act(async () => {
      await useSessionStore.getState().create(bridge, {
        harness: "mock",
        cwd: "C:/work/grove",
        config: { command: "C:/tools/mock.exe", args: ["--fast"] },
      });
    });

    // `config` is the adapter-specific escape hatch (`config.command`/`config.args`); the form does
    // not set it yet, but the command has to carry it so a caller can.
    expect(bridge.calls).toContainEqual({
      command: "session_create",
      args: {
        req: {
          harness: "mock",
          cwd: "C:/work/grove",
          config: { command: "C:/tools/mock.exe", args: ["--fast"] },
        },
      },
    });
  });

  it("omits an unset adapter config so the request stays minimal", async () => {
    const bridge = createFakeBridge();
    renderPanel(bridge);
    await screen.findByTestId("harness-picker");

    await act(async () => {
      await useSessionStore.getState().create(bridge, { harness: "mock" });
    });

    expect(bridge.calls).toContainEqual({
      command: "session_create",
      args: { req: { harness: "mock" } },
    });
  });

  it("streams live events into the open conversation", async () => {
    const row = detail("ready");
    const bridge = createFakeBridge({}, { sessions: [fakeSummary(row)], details: { [row.id]: row } });
    renderPanel(bridge);
    await userEvent.click(await screen.findByTestId("session-item-session-1"));

    act(() => {
      bridge.emitSessionEvent({ sessionId: row.id, payload: { at: AT, kind: "delta", text: "Mock " } });
      bridge.emitSessionEvent({ sessionId: row.id, payload: { at: AT, kind: "delta", text: "reply" } });
    });
    expect(await screen.findByTestId("transcript-stream")).toHaveTextContent("Mock reply");

    act(() => {
      bridge.emitSessionEvent({
        sessionId: row.id,
        payload: {
          at: AT,
          kind: "message",
          role: "assistant",
          blocks: [{ type: "text", text: "Mock reply" }],
        },
      });
    });

    await waitFor(() =>
      expect(screen.queryByTestId("transcript-stream")).not.toBeInTheDocument(),
    );
    expect(screen.getByTestId("transcript-message")).toHaveTextContent("Mock reply");
  });

  it("renders a tool call as one card with its result", async () => {
    const row = detail("idle", [
      { at: AT, kind: "tool_call", call_id: "call-1", name: "echo", title: "echo hello" },
      {
        at: AT,
        kind: "tool_result",
        call_id: "call-1",
        status: "ok",
        summary: "echoed 5 chars",
      },
    ]);
    const bridge = createFakeBridge({}, { sessions: [fakeSummary(row, 2)], details: { [row.id]: row } });
    renderPanel(bridge);
    await userEvent.click(await screen.findByTestId("session-item-session-1"));

    const cards = await screen.findAllByTestId("transcript-tool-call");
    expect(cards).toHaveLength(1);
    expect(cards[0]).toHaveTextContent("echo");
    expect(cards[0]).toHaveTextContent("echo hello");
    expect(cards[0]).toHaveTextContent("ok");
    expect(cards[0]).toHaveTextContent("echoed 5 chars");
  });

  it("makes the active session's directory the project root", async () => {
    const row = detail("idle");
    row.cwd = "C:/work/grove";
    const bridge = createFakeBridge({}, { sessions: [fakeSummary(row)], details: { [row.id]: row } });
    renderPanel(bridge);
    await userEvent.click(await screen.findByTestId("session-item-session-1"));

    await waitFor(() =>
      expect(bridge.calls).toContainEqual({
        command: "app_set_project_root",
        args: { path: "C:/work/grove" },
      }),
    );
    expect(bridge.projectRoot()).toBe("C:/work/grove");
  });

  it("answers a permission request and drops the buttons once it resolves", async () => {
    const row = detail("busy", [
      { at: AT, kind: "permission_request", request_id: "req-1", title: "Run tests", detail: "npm test" },
    ]);
    const bridge = createFakeBridge({}, {
      sessions: [fakeSummary(row, 1)],
      details: { [row.id]: row },
    });
    renderPanel(bridge);
    await userEvent.click(await screen.findByTestId("session-item-session-1"));

    const card = await screen.findByTestId("transcript-permission");
    expect(card).toHaveTextContent("Run tests");
    expect(card).toHaveTextContent("npm test");
    expect(screen.getByTestId("transcript-permission-state")).toHaveTextContent("pending");

    await userEvent.click(screen.getByTestId("permission-approve"));

    await waitFor(() =>
      expect(bridge.calls).toContainEqual({
        command: "session_respond_permission",
        args: { id: row.id, requestId: "req-1", decision: "approve" },
      }),
    );
    // The fake answers the request exactly like an adapter does: the resolution is the event that
    // removes the buttons, not the click.
    await waitFor(() =>
      expect(screen.queryByTestId("permission-approve")).not.toBeInTheDocument(),
    );
    expect(await screen.findByTestId("transcript-permission")).toHaveTextContent("approve");
  });

  it("denies a permission request", async () => {
    const row = detail("busy", [
      { at: AT, kind: "permission_request", request_id: "req-2", title: "Delete files", detail: "" },
    ]);
    const bridge = createFakeBridge({}, {
      sessions: [fakeSummary(row, 1)],
      details: { [row.id]: row },
    });
    renderPanel(bridge);
    await userEvent.click(await screen.findByTestId("session-item-session-1"));

    await userEvent.click(await screen.findByTestId("permission-deny"));

    await waitFor(() =>
      expect(bridge.calls).toContainEqual({
        command: "session_respond_permission",
        args: { id: row.id, requestId: "req-2", decision: "deny" },
      }),
    );
    await waitFor(() =>
      expect(screen.queryByTestId("permission-deny")).not.toBeInTheDocument(),
    );
  });

  it("sends the compose text and cancels the run", async () => {
    const row = detail("idle");
    const bridge = createFakeBridge({}, { sessions: [fakeSummary(row)], details: { [row.id]: row } });
    renderPanel(bridge);
    await userEvent.click(await screen.findByTestId("session-item-session-1"));

    const compose = await screen.findByTestId("session-compose");
    await userEvent.type(compose, "hello harness");
    await userEvent.click(screen.getByTestId("session-send"));

    await waitFor(() =>
      expect(bridge.calls).toContainEqual({
        command: "session_send",
        args: { id: row.id, text: "hello harness" },
      }),
    );
    await waitFor(() => expect(compose).toHaveValue(""));

    await userEvent.click(screen.getByTestId("session-cancel"));
    await waitFor(() =>
      expect(bridge.calls.some((call) => call.command === "session_cancel")).toBe(true),
    );
  });

  it("updates the list and header from live status events", async () => {
    const row = detail("idle");
    const bridge = createFakeBridge({}, { sessions: [fakeSummary(row)], details: { [row.id]: row } });
    renderPanel(bridge);
    const item = await screen.findByTestId("session-item-session-1");
    await userEvent.click(item);

    act(() => {
      bridge.emitSessionEvent({ sessionId: row.id, payload: { at: AT, kind: "status", state: "busy" } });
    });

    await waitFor(() => expect(item).toHaveTextContent("busy"));
    expect(screen.getByTestId("session-main")).toHaveTextContent("busy");
  });

  it("offers Resume for a session that is not running, and resume refreshes the transcript", async () => {
    const row = detail("exited", [
      { at: AT, kind: "message", role: "user", blocks: [{ type: "text", text: "before restart" }] },
    ]);
    const bridge = createFakeBridge({}, { sessions: [fakeSummary(row, 1)], details: { [row.id]: row } });
    renderPanel(bridge);
    await userEvent.click(await screen.findByTestId("session-item-session-1"));

    await userEvent.click(await screen.findByTestId("session-resume"));

    await waitFor(() =>
      expect(bridge.calls.some((call) => call.command === "session_resume")).toBe(true),
    );
    expect(await screen.findByText("before restart")).toBeInTheDocument();
  });

  it("keeps two sessions open as tabs with their own transcript, compose text and status", async () => {
    const first = detail("idle", [
      { at: AT, kind: "message", role: "user", blocks: [{ type: "text", text: "hello one" }] },
    ]);
    const second: SessionDetail = {
      ...fakeSession({ id: "session-2", title: "second session", status: "busy" }),
      events: [{ at: AT, kind: "message", role: "user", blocks: [{ type: "text", text: "hello two" }] }],
    };
    const bridge = createFakeBridge({}, {
      sessions: [fakeSummary(first), fakeSummary(second)],
      details: { [first.id]: first, [second.id]: second },
    });
    renderPanel(bridge);

    await userEvent.click(await screen.findByTestId("session-item-session-1"));
    expect(await screen.findByTestId("session-tab-session-1")).toBeInTheDocument();
    expect(await screen.findByText("hello one")).toBeInTheDocument();
    await userEvent.type(screen.getByTestId("session-compose"), "draft one");

    await userEvent.click(screen.getByTestId("session-item-session-2"));
    expect(await screen.findByTestId("session-tab-session-2")).toBeInTheDocument();
    expect(await screen.findByText("hello two")).toBeInTheDocument();
    expect(screen.queryByText("hello one")).not.toBeInTheDocument();
    expect(screen.getByTestId("session-compose")).toHaveValue("");
    await userEvent.type(screen.getByTestId("session-compose"), "draft two");

    // Each tab keeps its own compose text and status.
    await userEvent.click(screen.getByTestId("session-tab-session-1"));
    expect(await screen.findByTestId("session-compose")).toHaveValue("draft one");
    expect(
      screen.getByTestId("session-tab-session-2").querySelector(".status-dot"),
    ).toHaveClass("status-busy");
    expect(screen.queryByText("hello two")).not.toBeInTheDocument();

    // Closing a tab keeps the session itself and focuses a neighbour.
    await userEvent.click(screen.getByTestId("session-tab-close-session-1"));
    await waitFor(() =>
      expect(screen.queryByTestId("session-tab-session-1")).not.toBeInTheDocument(),
    );
    expect(await screen.findByTestId("session-tab-session-2")).toBeInTheDocument();
    expect(screen.getByTestId("session-item-session-1")).toBeInTheDocument();
  });

  it("shows the session's pinned specs and plan in the header", async () => {
    const row = detail("idle", [
      { at: AT, kind: "message", role: "user", blocks: [{ type: "text", text: "hello" }] },
    ]);
    const bridge = createFakeBridge({}, {
      sessions: [fakeSummary(row)],
      details: { [row.id]: row },
      pins: { [row.id]: ["grove-architecture"] },
      specTitles: { "grove-architecture": "Grove architecture" },
      plans: {
        [row.id]: {
          sessionId: row.id,
          markdown: "# Plan\n\n- one\n",
          version: 3,
          mirrorPath: "C:/projects/grove/.grove/sessions/session-1/plan.md",
        },
      },
    });
    renderPanel(bridge);
    await userEvent.click(await screen.findByTestId("session-item-session-1"));

    const context = await screen.findByTestId("session-context");
    expect(within(context).getByTestId("session-context-pin")).toHaveTextContent(
      "Grove architecture",
    );
    expect(within(context).getByTestId("session-context-plan")).toHaveTextContent("plan v3");
    expect(within(context).getByTestId("session-context-size")).toHaveTextContent("2 docs");
  });

  it("switches harness without losing the transcript", async () => {
    const row = detail("idle", [
      { at: AT, kind: "message", role: "user", blocks: [{ type: "text", text: "hello claude" }] },
    ]);
    const bridge = createFakeBridge(
      {},
      {
        harnesses: [
          { id: "mock", capabilities: MOCK_CAPABILITIES },
          { id: "claude", capabilities: MOCK_CAPABILITIES },
        ],
        sessions: [fakeSummary(row)],
        details: { [row.id]: row },
        pins: { [row.id]: ["grove-architecture"] },
        specTitles: { "grove-architecture": "Grove architecture" },
      },
    );
    renderPanel(bridge);
    await userEvent.click(await screen.findByTestId("session-item-session-1"));
    expect(await screen.findByText("hello claude")).toBeInTheDocument();

    await userEvent.selectOptions(screen.getByTestId("session-harness"), "claude");

    await waitFor(() =>
      expect(bridge.calls).toContainEqual({
        command: "session_switch_harness",
        args: { id: row.id, harness: "claude" },
      }),
    );
    expect(await screen.findByText("hello claude")).toBeInTheDocument();
    expect(
      await screen.findByText(/Harness switched from mock to claude/),
    ).toBeInTheDocument();
    expect(screen.getByTestId("session-context-pin")).toHaveTextContent("Grove architecture");
  });

  it("refreshes the context and shows the note in the transcript", async () => {
    const row = detail("idle");
    const bridge = createFakeBridge({}, {
      sessions: [fakeSummary(row)],
      details: { [row.id]: row },
      pins: { [row.id]: ["grove-architecture"] },
      specTitles: { "grove-architecture": "Grove architecture" },
    });
    renderPanel(bridge);
    await userEvent.click(await screen.findByTestId("session-item-session-1"));

    await userEvent.click(await screen.findByTestId("session-refresh-context"));

    expect(await screen.findByTestId("transcript-notice")).toHaveTextContent(
      /Context refreshed for mock/,
    );
    expect(bridge.calls.some((call) => call.command === "session_refresh_context")).toBe(true);
  });

  it("shows a validation/transport error from the core instead of throwing", async () => {
    const bridge = createFakeBridge({
      invoke: async () => {
        throw { code: "unavailable", message: "session is not running" };
      },
    });
    renderPanel(bridge);

    expect(await screen.findByTestId("session-error")).toHaveTextContent("session is not running");
  });
});
