import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SessionEvent } from "@grove/contract";
import {
  DEFAULT_PROVIDERS,
  createFakeBridge,
  fakeProviderStatus,
  fakeSession,
  fakeSummary,
} from "../test-utils/fakeBridge";
import { useSessionStore } from "./sessionStore";

describe("session store model refresh", () => {
  beforeEach(() => {
    useSessionStore.getState().reset();
  });

  it("replaces the preset model list with the provider's live models", async () => {
    const bridge = createFakeBridge(
      {},
      {
        providers: DEFAULT_PROVIDERS,
        providerStatuses: [
          fakeProviderStatus({
            id: "mock",
            name: "Offline",
            models: ["mock-1"],
            defaultModel: "mock-1",
            configured: true,
          }),
        ],
      },
    );
    bridge.on("provider_models", () => ["mock-1", "mock-2"]);
    bridge.on("provider_thinking", () => ({ mock: { "mock-1": ["low", "high"] } }));

    await useSessionStore.getState().load(bridge);

    await vi.waitFor(() => {
      expect(useSessionStore.getState().providers[0].models).toEqual(["mock-1", "mock-2"]);
    });
    expect(useSessionStore.getState().providerStatuses[0].models).toEqual(["mock-1", "mock-2"]);
    expect(useSessionStore.getState().modelThinking).toEqual({
      mock: { "mock-1": ["low", "high"] },
    });
  });

  it("moves the default off a preset the provider no longer lists", async () => {
    const bridge = createFakeBridge(
      {},
      {
        providers: DEFAULT_PROVIDERS,
        providerStatuses: [
          fakeProviderStatus({
            id: "mock",
            name: "Offline",
            models: ["retired-model"],
            defaultModel: "retired-model",
            configured: true,
          }),
        ],
      },
    );
    bridge.on("provider_models", () => ["mock-2", "mock-3"]);

    await useSessionStore.getState().load(bridge);

    await vi.waitFor(() => {
      expect(useSessionStore.getState().providers[0].defaultModel).toBe("mock-2");
    });
    expect(useSessionStore.getState().providerStatuses[0].defaultModel).toBe("mock-2");
  });
});

describe("session store subagents", () => {
  const start: SessionEvent = {
    at: "2025-01-01T00:00:00.000Z",
    kind: "tool_call",
    call_id: "call-1",
    name: "subagent",
    title: "subagent · read the store",
  };

  beforeEach(() => {
    useSessionStore.getState().reset();
    useSessionStore.setState({
      sessions: [fakeSummary(fakeSession({ id: "s1", title: "Store audit" }))],
      activeId: "s1",
    });
  });

  it("tracks a worker from its tool call to its result", () => {
    useSessionStore.getState().applyEvent("s1", start);
    expect(useSessionStore.getState().sessions[0].subagents[0]).toMatchObject({
      callId: "call-1",
      description: "read the store",
      status: "running",
    });

    useSessionStore.getState().applyEvent("s1", {
      at: "2025-01-01T00:00:05.000Z",
      kind: "tool_result",
      call_id: "call-1",
      status: "ok",
      summary: "the report",
    });
    expect(useSessionStore.getState().sessions[0].subagents[0]).toMatchObject({
      status: "ok",
      summary: "the report",
      finishedAt: "2025-01-01T00:00:05.000Z",
    });
  });

  it("closes a running worker when the turn ends without a result", () => {
    useSessionStore.getState().applyEvent("s1", start);
    useSessionStore.getState().applyEvent("s1", {
      at: "2025-01-01T00:00:05.000Z",
      kind: "status",
      state: "idle",
    });
    expect(useSessionStore.getState().sessions[0].subagents[0]).toMatchObject({
      status: "cancelled",
      summary: "the turn ended before this worker finished",
    });
  });
});
