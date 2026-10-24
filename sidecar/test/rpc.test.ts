import { describe, expect, it } from "vitest";
import {
  handleRequest,
  PROTOCOL_VERSION,
  RPC_INVALID_REQUEST,
  RPC_METHOD_NOT_FOUND,
  RPC_UNAVAILABLE,
  type JsonRpcResponse,
} from "../src/rpc";

function request(method: string, params?: unknown, id: string | number = 1): unknown {
  return { jsonrpc: "2.0", id, method, params };
}

describe("sidecar rpc", () => {
  it("answers initialize with the protocol version", () => {
    const response = handleRequest(request("initialize", { protocolVersion: 1 })) as JsonRpcResponse;
    expect(response.error).toBeUndefined();
    expect(response.result).toMatchObject({ protocolVersion: PROTOCOL_VERSION, version: "0.1.0" });
  });

  it("rejects an unknown method with the JSON-RPC code", () => {
    const response = handleRequest(request("does.not.exist")) as JsonRpcResponse;
    expect(response.error?.code).toBe(RPC_METHOD_NOT_FOUND);
  });

  it("reports session methods as unavailable in this build", () => {
    const response = handleRequest(request("session.start", { sessionId: "s1" })) as JsonRpcResponse;
    expect(response.error?.code).toBe(RPC_UNAVAILABLE);
  });

  it("rejects a non-request payload", () => {
    expect((handleRequest({ nope: true }) as JsonRpcResponse).error?.code).toBe(RPC_INVALID_REQUEST);
    expect((handleRequest(null) as JsonRpcResponse).error?.code).toBe(RPC_INVALID_REQUEST);
  });

  it("keeps the request id so responses can be matched", () => {
    const response = handleRequest(request("initialize", undefined, "abc")) as JsonRpcResponse;
    expect(response.id).toBe("abc");
  });
});
