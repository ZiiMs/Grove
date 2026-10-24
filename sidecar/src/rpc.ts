/**
 * Grove Sidecar Protocol v1 — the request handler.
 *
 * Newline-delimited JSON-RPC 2.0 over stdio (see `docs/sidecar-protocol.md`). This module is pure so
 * tests drive it without a process; `main.ts` only wires stdin/stdout.
 */

export const PROTOCOL_VERSION = 1;
export const SIDECAR_VERSION = "0.1.0";

export type JsonRpcId = string | number | null;

export interface JsonRpcRequest {
  jsonrpc: "2.0";
  id: JsonRpcId;
  method: string;
  params?: unknown;
}

export interface JsonRpcError {
  code: number;
  message: string;
  data?: unknown;
}

export interface JsonRpcResponse {
  jsonrpc: "2.0";
  id: JsonRpcId;
  result?: unknown;
  error?: JsonRpcError;
}

export interface JsonRpcNotification {
  jsonrpc: "2.0";
  method: string;
  params?: unknown;
}

/** JSON-RPC reserved codes we use. */
export const RPC_PARSE_ERROR = -32700;
export const RPC_INVALID_REQUEST = -32600;
export const RPC_METHOD_NOT_FOUND = -32601;
export const RPC_INVALID_PARAMS = -32602;
export const RPC_UNAVAILABLE = -32000;

export function rpcOk(id: JsonRpcId, result: unknown): JsonRpcResponse {
  return { jsonrpc: "2.0", id, result };
}

export function rpcError(id: JsonRpcId, code: number, message: string, data?: unknown): JsonRpcResponse {
  return { jsonrpc: "2.0", id, error: { code, message, data } };
}

function isRequest(value: unknown): value is JsonRpcRequest {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Record<string, unknown>;
  return (
    candidate.jsonrpc === "2.0" &&
    (typeof candidate.id === "string" || typeof candidate.id === "number" || candidate.id === null) &&
    typeof candidate.method === "string"
  );
}

/**
 * Handle one decoded message. Returns the response to write, or `null` for notifications we accept
 * without answering.
 *
 * `session.*` handlers are added by the pi integration ticket; until then the sidecar answers the
 * handshake and reports the SDK as unavailable, which is a valid (and tested) state.
 */
export function handleRequest(message: unknown): JsonRpcResponse {
  if (!isRequest(message)) {
    return rpcError(null, RPC_INVALID_REQUEST, "expected a JSON-RPC 2.0 request with id and method");
  }

  const { id, method } = message;

  switch (method) {
    case "initialize":
      return rpcOk(id, {
        protocolVersion: PROTOCOL_VERSION,
        version: SIDECAR_VERSION,
        harnesses: [],
        pi: { available: false, reason: "pi SDK integration lands with ticket 13" },
      });
    case "session.start":
    case "session.send":
    case "session.permission":
    case "session.cancel":
    case "session.stop":
      return rpcError(id, RPC_UNAVAILABLE, `${method} is not available in this build`);
    default:
      return rpcError(id, RPC_METHOD_NOT_FOUND, `unknown method: ${method}`);
  }
}
