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
