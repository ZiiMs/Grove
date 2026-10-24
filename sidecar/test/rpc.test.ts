import { describe, expect, it } from "vitest";
import {
  handleRequest,
  PROTOCOL_VERSION,
  RPC_INVALID_PARAMS,
  RPC_INVALID_REQUEST,
  RPC_METHOD_NOT_FOUND,
  RPC_UNAVAILABLE,
  SidecarFailure,
  type JsonRpcResponse,
  type SidecarHost,
} from "../src/rpc";

function request(method: string, params?: unknown, id: string | number = 1): unknown {
  return { jsonrpc: "2.0", id, method, params };
}

/** A host whose only job is to say "yes" so the protocol layer can be tested alone. */
function host(overrides: Partial<SidecarHost> = {}): SidecarHost {
  return {
    info: () => ({
      protocolVersion: PROTOCOL_VERSION,
      version: "0.1.0",
      harnesses: ["pi"],
      pi: { available: true, command: "pi" },
    }),
    start: async () => ({}),
    send: async () => {},
    permission: async () => {},
    cancel: async () => {},
    stop: async () => {},
    shutdown: async () => {},
    ...overrides,
  };
}

async function call(message: unknown, hostImpl = host()): Promise<JsonRpcResponse> {
  const response = await handleRequest(message, hostImpl);
  if (!response) throw new Error("expected a response");
  return response;
}

describe("sidecar rpc", () => {
  it("answers initialize with the protocol version and the harness list", async () => {
    const response = await call(request("initialize", { protocolVersion: 1 }));
    expect(response.error).toBeUndefined();
    expect(response.result).toMatchObject({
      protocolVersion: PROTOCOL_VERSION,
      version: "0.1.0",
      harnesses: ["pi"],
    });
  });

  it("rejects an unknown method with the JSON-RPC code", async () => {
    const response = await call(request("does.not.exist"));
    expect(response.error?.code).toBe(RPC_METHOD_NOT_FOUND);
  });

  it("reports session.start as unavailable when the host cannot launch pi", async () => {
    const response = await call(
      request("session.start", { sessionId: "s1", cwd: "C:/work" }),
      host({
        start: async () => {
          throw new SidecarFailure(RPC_UNAVAILABLE, "pi is not installed", {
            reason: "pi is not installed",
          });
        },
      }),
    );
    expect(response.error?.code).toBe(RPC_UNAVAILABLE);
    expect(response.error?.data).toEqual({ reason: "pi is not installed" });
  });

  it("rejects a session request without a session id", async () => {
    const response = await call(request("session.send", { text: "hi" }));
    expect(response.error?.code).toBe(RPC_INVALID_PARAMS);
  });

  it("rejects a non-request payload", async () => {
    expect((await call({ nope: true })).error?.code).toBe(RPC_INVALID_REQUEST);
    expect((await call(null)).error?.code).toBe(RPC_INVALID_REQUEST);
  });

  it("keeps the request id so responses can be matched", async () => {
    const response = await call(request("initialize", undefined, "abc"));
    expect(response.id).toBe("abc");
  });
});
