/**
 * Grove Sidecar Protocol v1 — JSON-RPC 2.0 over newline-delimited JSON.
 *
 * `docs/sidecar-protocol.md` is the specification; this module is the request dispatcher and it is
 * deliberately pure. `main.ts`/`sidecar.ts` own the stdio plumbing and `pi_host.ts` owns the harness,
 * so the protocol can be tested without a process and a second harness can be added by implementing
 * [`SidecarHost`].
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

/** JSON-RPC reserved codes, plus the two this protocol adds for a harness that cannot start. */
export const RPC_PARSE_ERROR = -32700;
export const RPC_INVALID_REQUEST = -32600;
export const RPC_METHOD_NOT_FOUND = -32601;
export const RPC_INVALID_PARAMS = -32602;
export const RPC_INTERNAL = -32603;
/** The harness (or the sidecar's ability to launch it) is not available. */
export const RPC_UNAVAILABLE = -32000;
/** The harness does not support the operation at all (for example pi permissions). */
export const RPC_UNSUPPORTED = -32001;
/** No such session id. */
export const RPC_NOT_FOUND = -32002;

export function rpcOk(id: JsonRpcId, result: unknown): JsonRpcResponse {
  return { jsonrpc: "2.0", id, result };
}

export function rpcError(id: JsonRpcId, code: number, message: string, data?: unknown): JsonRpcResponse {
  return { jsonrpc: "2.0", id, error: { code, message, data } };
}

/** A failure a host raises with the exact JSON-RPC code the caller should see. */
export class SidecarFailure extends Error {
  constructor(
    public readonly code: number,
    message: string,
    public readonly data?: unknown,
  ) {
    super(message);
    this.name = "SidecarFailure";
  }
}

/** What `initialize` reports about pi. */
export interface PiAvailability {
  available: boolean;
  /** The command that was resolved (or looked for), so a user can see what was tried. */
  command?: string;
  /** Why it is unavailable. Present only when `available` is false. */
  reason?: string;
}

export interface InitializeResult {
  protocolVersion: number;
  version: string;
  /** Harness ids this sidecar can drive. */
  harnesses: string[];
  pi: PiAvailability;
}

/** How to launch pi. `config.pi` in `session.start`; the sidecar's own defaults otherwise. */
export interface PiLaunchConfig {
  command?: string;
  args?: string[];
  sessionDir?: string;
}

export interface SessionDoc {
  id: string;
  title: string;
  body: string;
}

export interface SessionStartParams {
  sessionId: string;
  cwd: string;
  model?: string;
  /** An initial user prompt; the sidecar queues it as the first turn. */
  prompt?: string;
  /** pi's session **file path** (`data.sessionFile` from `get_session_stats`). */
  resumeToken?: string;
  config?: Record<string, unknown> & { pi?: PiLaunchConfig };
  /** Pinned specs and the plan, already rendered to markdown. */
  context?: SessionDoc[];
}

export interface SessionStartResult {
  /** The session file path, so Rust can mirror it into `sessions.resume_token`. */
  resumeToken?: string;
}

export interface SessionSendParams {
  sessionId: string;
  text: string;
}

export interface SessionPermissionParams {
  sessionId: string;
  requestId: string;
  decision: "approve" | "deny";
}

export interface SessionRef {
  sessionId: string;
}

/**
 * One harness behind the protocol. Every method either resolves with the JSON-RPC result or throws
 * a [`SidecarFailure`], which carries its own code and reason.
 */
export interface SidecarHost {
  info(): InitializeResult;
  start(params: SessionStartParams): Promise<SessionStartResult>;
  send(params: SessionSendParams): Promise<void>;
  permission(params: SessionPermissionParams): Promise<void>;
  cancel(params: SessionRef): Promise<void>;
  stop(params: SessionRef): Promise<void>;
  /** The parent is gone: release every child process. */
  shutdown(): Promise<void>;
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

function paramsObject(params: unknown): Record<string, unknown> {
  if (typeof params !== "object" || params === null || Array.isArray(params)) {
    throw new SidecarFailure(RPC_INVALID_PARAMS, "params must be an object");
  }
  return params as Record<string, unknown>;
}

function requiredString(params: Record<string, unknown>, key: string): string {
  const value = params[key];
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new SidecarFailure(RPC_INVALID_PARAMS, `params.${key} must be a non-empty string`);
  }
  return value;
}

function optionalString(params: Record<string, unknown>, key: string): string | undefined {
  const value = params[key];
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function sessionRef(params: unknown): SessionRef {
  const object = paramsObject(params);
  return { sessionId: requiredString(object, "sessionId") };
}

/**
 * Handle one decoded message. Returns the response to write, or `null` for notifications, which the
 * protocol accepts without answering.
 */
export async function handleRequest(
  message: unknown,
  host: SidecarHost,
): Promise<JsonRpcResponse | null> {
  if (!isRequest(message)) {
    return rpcError(null, RPC_INVALID_REQUEST, "expected a JSON-RPC 2.0 request with id and method");
  }

  const { id, method, params } = message;

  try {
    switch (method) {
      case "initialize": {
        return rpcOk(id, host.info());
      }
      case "session.start": {
        const object = paramsObject(params);
        const start: SessionStartParams = {
          sessionId: requiredString(object, "sessionId"),
          cwd: requiredString(object, "cwd"),
          model: optionalString(object, "model"),
          prompt: optionalString(object, "prompt"),
          resumeToken: optionalString(object, "resumeToken"),
          config: (typeof object.config === "object" && object.config !== null
            ? (object.config as Record<string, unknown>)
            : {}) as SessionStartParams["config"],
          context: Array.isArray(object.context) ? (object.context as SessionDoc[]) : [],
        };
        return rpcOk(id, await host.start(start));
      }
      case "session.send": {
        const object = paramsObject(params);
        await host.send({
          sessionId: requiredString(object, "sessionId"),
          text: requiredString(object, "text"),
        });
        return rpcOk(id, {});
      }
      case "session.permission": {
        const object = paramsObject(params);
        const decision = requiredString(object, "decision");
        if (decision !== "approve" && decision !== "deny") {
          throw new SidecarFailure(RPC_INVALID_PARAMS, "params.decision must be approve or deny");
        }
        await host.permission({
          sessionId: requiredString(object, "sessionId"),
          requestId: requiredString(object, "requestId"),
          decision,
        });
        return rpcOk(id, {});
      }
      case "session.cancel": {
        await host.cancel(sessionRef(params));
        return rpcOk(id, {});
      }
      case "session.stop": {
        await host.stop(sessionRef(params));
        return rpcOk(id, {});
      }
      default:
        return rpcError(id, RPC_METHOD_NOT_FOUND, `unknown method: ${method}`);
    }
  } catch (error) {
    if (error instanceof SidecarFailure) {
      return rpcError(id, error.code, error.message, error.data);
    }
    return rpcError(id, RPC_INTERNAL, error instanceof Error ? error.message : String(error));
  }
}
