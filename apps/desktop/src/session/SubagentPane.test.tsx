import type { SessionEvent } from "@grove/contract";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { BridgeProvider } from "../ipc/context";
import { useSessionStore } from "../state/sessionStore";
import { createFakeBridge, fakeSession, fakeSummary } from "../test-utils/fakeBridge";
import { SubagentPane } from "./SubagentPane";

const at = "2025-01-01T00:00:00.000Z";
const session = fakeSession({ id: "s1", title: "Store audit", provider: "mock" });
const events: SessionEvent[] = [
  {
    at,
    kind: "tool_call",
    call_id: "call-1",
    name: "subagent",
    title: "subagent · audit the store",
  },
  {
    at,
    kind: "tool_call",
    call_id: "sub-call-1-read",
    name: "read_file",
    title: "subagent · read_file crates/store.rs",
  },
  {
    at,
    kind: "tool_result",
    call_id: "sub-call-1-read",
    status: "ok",
    summary: "the store",
  },
  {
    at,
    kind: "tool_result",
    call_id: "call-1",
    status: "ok",
    summary: "worker done",
  },
];

beforeEach(() => {
  useSessionStore.getState().reset();
  useSessionStore.setState({
    sessions: [
      {
        ...fakeSummary(session),
        subagents: [
          {
            callId: "call-1",
            description: "audit the store",
            status: "ok",
            summary: 'subagent "audit the store" finished.\n\nThis is testing a sub agent',
            startedAt: at,
            finishedAt: at,
          },
        ],
      },
    ],
    activeId: "s1",
    detail: { ...session, thinkingLevel: "off", events },
    events,
  });
});

describe("SubagentPane", () => {
  it("shows the worker's forwarded rows under its own header", () => {
    render(
      <BridgeProvider bridge={createFakeBridge()}>
        <SubagentPane active sessionId="s1" callId="call-1" onClose={vi.fn()} />
      </BridgeProvider>,
    );

    expect(screen.getByRole("heading", { name: "audit the store" })).toBeInTheDocument();
    expect(screen.getByText("worker in Store audit")).toBeInTheDocument();
    expect(screen.getByText("Done")).toBeInTheDocument();
    expect(screen.getByText("This is testing a sub agent")).toBeInTheDocument();
    expect(screen.getByText("read_file")).toBeInTheDocument();
    expect(screen.getByText("crates/store.rs")).toBeInTheDocument();
  });

  it("returns to the parent session", async () => {
    const onClose = vi.fn();
    render(
      <BridgeProvider bridge={createFakeBridge()}>
        <SubagentPane active sessionId="s1" callId="call-1" onClose={onClose} />
      </BridgeProvider>,
    );

    await userEvent.click(screen.getByRole("button", { name: /Session/ }));
    expect(onClose).toHaveBeenCalled();
  });
});
