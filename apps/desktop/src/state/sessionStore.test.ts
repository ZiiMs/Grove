import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  DEFAULT_PROVIDERS,
  createFakeBridge,
  fakeProviderStatus,
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

    await useSessionStore.getState().load(bridge);

    await vi.waitFor(() => {
      expect(useSessionStore.getState().providers[0].models).toEqual(["mock-1", "mock-2"]);
    });
    expect(useSessionStore.getState().providerStatuses[0].models).toEqual(["mock-1", "mock-2"]);
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
