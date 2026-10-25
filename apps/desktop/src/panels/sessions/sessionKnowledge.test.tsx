import type { SpecDoc, SpecGraph, SpecNode } from "@grove/contract";
import { render, screen, waitFor, within } from "@testing-library/react";
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
import { SessionsPanel } from "./index";
import { SpecsPanel } from "../specs";

/**
 * The key user story of `goal-and-requirements.md`, end to end under `FakeBridge`: two sessions side
 * by side, a spec pinned from the Specs panel, the pinned context visible in the session header, a
 * harness switch that keeps the transcript, and a refresh that says so in the transcript.
 */

const AT = "2025-01-01T00:00:01.000Z";

const architectureNode: SpecNode = {
  id: "grove-architecture",
  type: "architecture-design",
  title: "Grove architecture",
  status: "active",
  path: "architecture.md",
  dependsOn: [],
  references: [],
  implements: [],
  covers: [],
};

const graph: SpecGraph = {
  root: "C:/demo/grove",
  nodes: [architectureNode],
  validation: { danglingLinks: [], duplicateIds: [], parentCycles: [] },
};

const docs: Record<string, SpecDoc> = {
  "architecture.md": {
    node: architectureNode,
    body: "# Grove architecture\n\nA Tauri shell over a Rust core.\n",
    hash: "hash-architecture-1",
  },
};

function bridgeWithTwoSessions(): FakeBridge {
  const first = {
    ...fakeSession({ id: "session-1", title: "claude session", status: "exited" as const }),
    events: [
      { at: AT, kind: "message" as const, role: "user" as const, blocks: [{ type: "text" as const, text: "first task" }] },
    ],
  };
  const second = {
    ...fakeSession({ id: "session-2", title: "codex session", status: "exited" as const }),
    events: [
      { at: AT, kind: "message" as const, role: "user" as const, blocks: [{ type: "text" as const, text: "second task" }] },
    ],
  };
  const bridge = createFakeBridge(
    {},
    {
      harnesses: [
        { id: "mock", capabilities: MOCK_CAPABILITIES },
        { id: "claude", capabilities: MOCK_CAPABILITIES },
      ],
      sessions: [fakeSummary(first, 1), fakeSummary(second, 1)],
      details: { "session-1": first, "session-2": second },
      specTitles: { "grove-architecture": "Grove architecture" },
    },
  );
  bridge.on("spec_index", () => graph);
  bridge.on("spec_read", ({ path }) => {
    const doc = docs[path];
    if (!doc) throw { code: "not_found", message: `no spec at ${path}` };
    return doc;
  });
  return bridge;
}

beforeEach(() => {
  useSessionStore.getState().reset();
});

describe("session knowledge", () => {
  it("pins a spec to one of two open sessions, resumes with it, switches harness and refreshes", async () => {
    const bridge = bridgeWithTwoSessions();
    render(
      <BridgeProvider bridge={bridge}>
        <SessionsPanel />
        <SpecsPanel />
      </BridgeProvider>,
    );

    // Two sessions side by side: each open as a tab, only the active transcript rendered.
    await userEvent.click(await screen.findByTestId("session-item-session-1"));
    expect(await screen.findByText("first task")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("session-item-session-2"));
    expect(await screen.findByText("second task")).toBeInTheDocument();
    expect(screen.getByTestId("session-tab-session-1")).toBeInTheDocument();
    expect(screen.getByTestId("session-tab-session-2")).toBeInTheDocument();
    expect(screen.queryByText("first task")).not.toBeInTheDocument();

    // Pin a spec to the active session from the Specs panel.
    await userEvent.click(await screen.findByRole("button", { name: /Grove architecture/ }));
    await screen.findByTestId("spec-reader");
    await userEvent.click(screen.getByTestId("spec-pin"));
    await waitFor(() => expect(bridge.pinsOf("session-2")).toEqual(["grove-architecture"]));

    // The header shows the pinned spec before the session is even started.
    const context = await screen.findByTestId("session-context");
    expect(within(context).getByTestId("session-context-pin")).toHaveTextContent(
      "Grove architecture",
    );
    expect(screen.getByTestId("session-context-size")).toHaveTextContent("1 docs");

    // Start it: the harness receives the pinned spec, and the transcript says so.
    await userEvent.click(screen.getByTestId("session-resume"));
    expect(await screen.findByText(/Context re-sent on resume to mock/)).toBeInTheDocument();
    expect(screen.getByText("second task")).toBeInTheDocument();

    // Switch harness: same conversation, same context, one visible note.
    await userEvent.selectOptions(screen.getByTestId("session-harness"), "claude");
    expect(await screen.findByText(/Harness switched from mock to claude/)).toBeInTheDocument();
    expect(screen.getByText("second task")).toBeInTheDocument();
    expect(screen.getByTestId("session-context-pin")).toHaveTextContent("Grove architecture");

    // Refresh context: it is re-read and announced, and the transcript is still intact.
    await userEvent.click(screen.getByTestId("session-refresh-context"));
    expect(await screen.findByText(/Context refreshed for claude/)).toBeInTheDocument();
    expect(screen.getByText("second task")).toBeInTheDocument();

    // Closing a tab does not end the session: the other tab takes focus.
    await userEvent.click(screen.getByTestId("session-tab-close-session-1"));
    await waitFor(() =>
      expect(screen.queryByTestId("session-tab-session-1")).not.toBeInTheDocument(),
    );
    expect(screen.getByTestId("session-item-session-1")).toBeInTheDocument();
  });
});
