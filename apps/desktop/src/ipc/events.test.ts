import { describe, expect, it, vi } from "vitest";
import { subscribeToSessionEvents } from "./events";
import { createFakeBridge } from "../test-utils/fakeBridge";

const knownStatus = {
  sessionId: "session-1",
  payload: { kind: "status", state: "idle", at: "2025-01-01T00:00:00Z" },
};

describe("session event stream", () => {
  it("delivers known events and stops after unlisten", async () => {
    const bridge = createFakeBridge();
    const handler = vi.fn();
    const unlisten = await subscribeToSessionEvents(bridge, handler);
    expect(bridge.listenerCount("grove://session-event")).toBe(1);

    bridge.emitSessionEvent(knownStatus);
    expect(handler).toHaveBeenCalledTimes(1);

    unlisten();
    expect(bridge.listenerCount("grove://session-event")).toBe(0);
  });

  it("ignores an unknown kind instead of throwing", async () => {
    const bridge = createFakeBridge();
    const handler = vi.fn();
    await subscribeToSessionEvents(bridge, handler);

    bridge.emitSessionEvent({ payload: { kind: "from-a-newer-core" } } as never);

    expect(handler).not.toHaveBeenCalled();
  });

  it("ignores a malformed payload instead of throwing", async () => {
    const bridge = createFakeBridge();
    const handler = vi.fn();
    await subscribeToSessionEvents(bridge, handler);

    bridge.emit("grove://session-event", { nope: true });
    bridge.emit("grove://session-event", null);

    expect(handler).not.toHaveBeenCalled();
  });
});
