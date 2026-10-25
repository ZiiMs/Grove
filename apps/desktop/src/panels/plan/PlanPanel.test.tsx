import type { Plan } from "@grove/contract";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { BridgeProvider } from "../../ipc/context";
import { useSessionStore } from "../../state/sessionStore";
import {
  createFakeBridge,
  fakeSession,
  fakeSummary,
  type FakeBridge,
  type FakeBridgeSeed,
} from "../../test-utils/fakeBridge";
import { PlanPanel } from ".";

/**
 * The Plan panel is exercised only through `FakeBridge`, the same typed bridge the app uses. The
 * fake really version-checks `plan_set`, so a conflict here is the conflict the core would return.
 */

function planFor(sessionId: string, overrides: Partial<Plan> = {}): Plan {
  return {
    sessionId,
    markdown: "# Plan\n\n- one\n",
    version: 1,
    mirrorPath: `C:/projects/grove/.grove/sessions/${sessionId}/plan.md`,
    ...overrides,
  };
}

function seed(plans: Record<string, Plan> = { "session-1": planFor("session-1") }): FakeBridge {
  const seedData: FakeBridgeSeed = {
    sessions: [
      fakeSummary(fakeSession({ id: "session-1", title: "First session" })),
      fakeSummary(fakeSession({ id: "session-2", title: "Second session" })),
    ],
    plans,
  };
  return createFakeBridge({}, seedData);
}

function renderPanel(bridge: FakeBridge) {
  render(
    <BridgeProvider bridge={bridge}>
      <PlanPanel />
    </BridgeProvider>,
  );
}

function editor(): HTMLTextAreaElement {
  return screen.getByTestId("plan-editor") as HTMLTextAreaElement;
}

async function waitForSubscription(bridge: FakeBridge) {
  await waitFor(() => expect(bridge.listenerCount("grove://plan-changed")).toBe(1));
}

afterEach(() => {
  useSessionStore.setState({ activeId: null });
});

describe("PlanPanel", () => {
  it("loads the first session's plan with its version and mirror path", async () => {
    const bridge = seed();
    renderPanel(bridge);

    await screen.findByTestId("plan-editor");
    expect(editor().value).toBe("# Plan\n\n- one\n");
    expect(screen.getByTestId("plan-version")).toHaveTextContent("version 1");
    expect(screen.getByTestId("plan-mirror-path")).toHaveTextContent(
      "C:/projects/grove/.grove/sessions/session-1/plan.md",
    );
    expect(screen.getByTestId("plan-copy-path")).toBeInTheDocument();
    expect(bridge.calls.some((call) => call.command === "plan_get")).toBe(true);
  });

  it("prefers the session the Sessions panel has open over the first row", async () => {
    useSessionStore.setState({ activeId: "session-2" });
    const bridge = seed({
      "session-1": planFor("session-1"),
      "session-2": planFor("session-2", { markdown: "second plan\n", version: 4 }),
    });
    renderPanel(bridge);

    await screen.findByTestId("plan-editor");
    expect(editor().value).toBe("second plan\n");
    expect(screen.getByTestId("plan-version")).toHaveTextContent("version 4");
  });

  it("saves the draft with the loaded version and shows the new version", async () => {
    const bridge = seed();
    renderPanel(bridge);
    await screen.findByTestId("plan-editor");

    fireEvent.change(editor(), { target: { value: "# Plan\n\n- one\n- two\n" } });
    expect(screen.getByTestId("plan-dirty")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("plan-save"));

    await waitFor(() => expect(screen.getByTestId("plan-version")).toHaveTextContent("version 2"));
    expect(bridge.calls.find((call) => call.command === "plan_set")?.args).toEqual({
      id: "session-1",
      update: { markdown: "# Plan\n\n- one\n- two\n", expectedVersion: 1 },
    });
    expect(editor().value).toBe("# Plan\n\n- one\n- two\n");
    expect(screen.queryByTestId("plan-dirty")).not.toBeInTheDocument();
    expect(screen.getByTestId("plan-preview")).toHaveTextContent("two");
  });

  it("a stale save surfaces the conflict and offers a reload instead of clobbering", async () => {
    const bridge = seed();
    renderPanel(bridge);
    await screen.findByTestId("plan-editor");

    fireEvent.change(editor(), { target: { value: "human draft\n" } });
    // The agent edits the mirror; the core imports it and moves the version.
    bridge.setPlan(planFor("session-1", { markdown: "agent plan\n", version: 2 }));
    await userEvent.click(screen.getByTestId("plan-save"));

    const banner = await screen.findByTestId("plan-conflict");
    expect(banner).toHaveTextContent("expected version 1");
    expect(editor().value).toBe("human draft\n");

    await userEvent.click(screen.getByTestId("plan-conflict-reload"));
    await waitFor(() => expect(editor().value).toBe("agent plan\n"));
    expect(screen.getByTestId("plan-version")).toHaveTextContent("version 2");
    expect(screen.queryByTestId("plan-conflict")).not.toBeInTheDocument();
  });

  it("renders the preview as markdown with HTML disabled", async () => {
    const bridge = seed({
      "session-1": planFor("session-1", {
        markdown: '# Title\n\n<script>alert("x")</script>\n\n- item\n',
      }),
    });
    renderPanel(bridge);

    const preview = await screen.findByTestId("plan-preview");
    expect(preview.querySelector("h1")?.textContent).toBe("Title");
    expect(preview.querySelector("li")?.textContent).toBe("item");
    expect(preview.querySelector("script")).toBeNull();
    expect(preview.innerHTML).toContain("&lt;script&gt;");
  });

  it("adopts an agent edit announced on grove://plan-changed when the draft is clean", async () => {
    const bridge = seed();
    renderPanel(bridge);
    await screen.findByTestId("plan-editor");
    await waitForSubscription(bridge);

    const agentPlan = planFor("session-1", { markdown: "agent plan\n", version: 2 });
    bridge.setPlan(agentPlan);
    act(() => {
      bridge.emit("grove://plan-changed", { sessionId: "session-1", payload: agentPlan });
    });

    await waitFor(() => expect(editor().value).toBe("agent plan\n"));
    expect(screen.getByTestId("plan-version")).toHaveTextContent("version 2");
  });

  it("keeps an unsaved draft and offers a reload when the agent edits first", async () => {
    const bridge = seed();
    renderPanel(bridge);
    await screen.findByTestId("plan-editor");
    await waitForSubscription(bridge);

    fireEvent.change(editor(), { target: { value: "my draft\n" } });
    const agentPlan = planFor("session-1", { markdown: "agent plan\n", version: 2 });
    bridge.setPlan(agentPlan);
    act(() => {
      bridge.emit("grove://plan-changed", { sessionId: "session-1", payload: agentPlan });
    });

    expect(await screen.findByTestId("plan-conflict")).toBeInTheDocument();
    expect(editor().value).toBe("my draft\n");

    await userEvent.click(screen.getByTestId("plan-conflict-reload"));
    await waitFor(() => expect(editor().value).toBe("agent plan\n"));
  });

  it("copies the mirror path", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    const bridge = seed();
    renderPanel(bridge);
    await screen.findByTestId("plan-copy-path");

    await userEvent.click(screen.getByTestId("plan-copy-path"));

    expect(writeText).toHaveBeenCalledWith(
      "C:/projects/grove/.grove/sessions/session-1/plan.md",
    );
    expect(await screen.findByTestId("plan-copy-note")).toHaveTextContent("Copied");
  });

  it("says so when there is no session to plan for", async () => {
    const bridge = createFakeBridge();
    renderPanel(bridge);

    expect(await screen.findByTestId("plan-empty")).toHaveTextContent(
      "Create a session to give it a plan.",
    );
    expect(screen.queryByTestId("plan-editor")).not.toBeInTheDocument();
  });
});
