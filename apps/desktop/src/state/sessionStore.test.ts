import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SessionDetail, SessionEvent } from "@grove/contract";
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

describe("session store model changes", () => {
  it("hot-swaps the active session's model without touching its transcript", async () => {
    useSessionStore.getState().reset();
    const detail: SessionDetail = {
      ...fakeSession({
        id: "s1",
        provider: "anthropic",
        model: "claude-sonnet-4-5",
      }),
      thinkingLevel: "off",
      events: [],
    };
    const bridge = createFakeBridge(
      {},
      {
        sessions: [fakeSummary(detail)],
        details: { [detail.id]: detail },
      },
    );
    useSessionStore.setState({
      activeId: detail.id,
      detail,
      events: [],
      sessions: [fakeSummary(detail)],
    });

    const changed = await useSessionStore
      .getState()
      .setModel(bridge, "claude-opus-4-1", "high");

    expect(changed).toBe(true);
    expect(useSessionStore.getState().detail?.model).toBe("claude-opus-4-1");
    expect(useSessionStore.getState().detail?.thinkingLevel).toBe("high");
    expect(useSessionStore.getState().sessions[0].model).toBe("claude-opus-4-1");
    expect(useSessionStore.getState().events).toEqual([]);
    expect(bridge.invocations).toContain("session_set_model");
    expect(bridge.invocations).not.toContain("session_switch_provider");
    expect(bridge.calls.find((call) => call.command === "session_set_model")?.args).toEqual({
      id: "s1",
      model: "claude-opus-4-1",
      thinking: "high",
    });
  });
});

describe("session store cross-provider switches", () => {
  const baseSession = (overrides: Partial<SessionDetail> = {}): SessionDetail => ({
    ...fakeSession({
      id: "s1",
      provider: "anthropic",
      model: "claude-sonnet-4-5",
      status: "idle",
    }),
    thinkingLevel: "off",
    events: [],
    ...overrides,
  });

  function seed(detail: SessionDetail) {
    useSessionStore.setState({
      activeId: detail.id,
      detail,
      events: [],
      sessions: [fakeSummary(detail)],
    });
  }

  function bridgeFor(detail: SessionDetail) {
    return createFakeBridge(
      {},
      {
        sessions: [fakeSummary(detail)],
        details: { [detail.id]: detail },
      },
    );
  }

  const status = (state: SessionDetail["status"]): SessionEvent => ({
    at: "2025-01-01T00:00:05.000Z",
    kind: "status",
    state,
  });

  beforeEach(() => {
    useSessionStore.getState().reset();
  });

  it("switches immediately when the session is idle", async () => {
    const detail = baseSession();
    const bridge = bridgeFor(detail);
    seed(detail);

    const landed = await useSessionStore
      .getState()
      .switchModel(bridge, "openai", "gpt-5-mini");

    expect(landed).toBe(true);
    expect(bridge.invocations).toContain("session_switch_provider");
    expect(
      bridge.calls.find((call) => call.command === "session_switch_provider")?.args,
    ).toMatchObject({ id: "s1", provider: "openai", model: "gpt-5-mini" });
    expect(useSessionStore.getState().pendingModel).toBeNull();
    expect(useSessionStore.getState().detail?.provider).toBe("openai");
  });

  it("queues a cross-provider switch while the turn is running", async () => {
    const detail = baseSession({ status: "busy" });
    const bridge = bridgeFor(detail);
    seed(detail);

    const landed = await useSessionStore
      .getState()
      .switchModel(bridge, "openai", "gpt-5-mini");

    expect(landed).toBe(false);
    expect(useSessionStore.getState().pendingModel).toEqual({
      sessionId: "s1",
      provider: "openai",
      model: "gpt-5-mini",
    });
    expect(bridge.invocations).not.toContain("session_switch_provider");
    expect(useSessionStore.getState().detail?.provider).toBe("anthropic");
  });

  it("keeps a queued switch waiting while a permission is open", async () => {
    const detail = baseSession({ status: "idle", awaiting: true });
    const bridge = bridgeFor(detail);
    seed(detail);

    await useSessionStore.getState().switchModel(bridge, "openai", "gpt-5-mini");
    const applied = await useSessionStore.getState().flushPendingModel(bridge);

    expect(applied).toBeNull();
    expect(bridge.invocations).not.toContain("session_switch_provider");
    expect(useSessionStore.getState().pendingModel).not.toBeNull();
  });

  it("applies the queued switch when the session reaches idle", async () => {
    const detail = baseSession({ status: "busy" });
    const bridge = bridgeFor(detail);
    seed(detail);
    await useSessionStore.getState().switchModel(bridge, "openai", "gpt-5-mini");

    // The status stream lands idle; the shell then retries the queued switch.
    useSessionStore.getState().applyEvent("s1", status("idle"));
    const applied = await useSessionStore.getState().flushPendingModel(bridge);

    expect(applied).toEqual({
      sessionId: "s1",
      provider: "openai",
      model: "gpt-5-mini",
    });
    expect(bridge.invocations).toContain("session_switch_provider");
    expect(useSessionStore.getState().pendingModel).toBeNull();
    expect(useSessionStore.getState().detail?.provider).toBe("openai");
    expect(useSessionStore.getState().sessions[0].provider).toBe("openai");
  });

  it("drops a queued switch when the session exits", async () => {
    const detail = baseSession({ status: "busy" });
    const bridge = bridgeFor(detail);
    seed(detail);
    await useSessionStore.getState().switchModel(bridge, "openai", "gpt-5-mini");

    useSessionStore.getState().applyEvent("s1", status("exited"));

    expect(useSessionStore.getState().pendingModel).toBeNull();
    expect(await useSessionStore.getState().flushPendingModel(bridge)).toBeNull();
    expect(bridge.invocations).not.toContain("session_switch_provider");
  });

  it("clears a pending switch when its session is archived", async () => {
    const detail = baseSession({ status: "busy" });
    const bridge = bridgeFor(detail);
    seed(detail);
    await useSessionStore.getState().switchModel(bridge, "openai", "gpt-5-mini");
    expect(useSessionStore.getState().pendingModel).not.toBeNull();

    expect(await useSessionStore.getState().archive(bridge, "s1")).toBe(true);

    expect(useSessionStore.getState().pendingModel).toBeNull();
  });

  it("clears a pending switch when its session is deleted", async () => {
    const detail = baseSession({ status: "busy" });
    const bridge = bridgeFor(detail);
    seed(detail);
    await useSessionStore.getState().switchModel(bridge, "openai", "gpt-5-mini");

    expect(await useSessionStore.getState().deleteSession(bridge, "s1")).toBe(true);

    expect(useSessionStore.getState().pendingModel).toBeNull();
    expect(useSessionStore.getState().sessions).toEqual([]);
  });
});

describe("session store subagents", () => {  const start: SessionEvent = {
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
