/**
 * The normalized event model, as the sidecar writes it onto the wire.
 *
 * Rust owns this shape (`crates/grove-core/src/sessions/event.rs`) and re-validates every event it
 * receives, dropping the ones it cannot deserialize — so a mismatch is a dropped event, never a
 * broken session. Fields stay snake_case (`call_id`, `input_tokens`) exactly like the Rust enum, and
 * the payload is flattened beside `at`/`kind` (`{ "at": "...", "kind": "delta", "text": "hi" }`).
 */

export type SessionState = "starting" | "ready" | "busy" | "idle" | "exited" | "error";
export type Role = "user" | "assistant" | "system";
export type ToolStatus = "ok" | "error" | "cancelled";
export type Decision = "approve" | "deny";

export type Block =
  | { type: "text"; text: string }
  | { type: "thinking"; text: string }
  | { type: "code"; language: string; text: string };

/** Every event kind and its payload, discriminated on `kind`. */
export type EventBody =
  | { kind: "session_started"; harness: string; model: string | null; resumed: boolean }
  | { kind: "delta"; text: string }
  | { kind: "message"; role: Role; blocks: Block[] }
  | { kind: "tool_call"; call_id: string; name: string; title: string }
  | { kind: "tool_result"; call_id: string; status: ToolStatus; summary: string }
  | { kind: "diff"; path: string; patch: string; added: number; removed: number }
  | { kind: "permission_request"; request_id: string; title: string; detail: string }
  | { kind: "permission_resolved"; request_id: string; decision: Decision }
  | { kind: "usage"; input_tokens: number; output_tokens: number }
  | { kind: "status"; state: SessionState }
  | { kind: "error"; message: string; retryable: boolean };

/** Distributes `at` across the union so `event.kind` narrows the payload. */
type Timestamped<T> = T extends unknown ? { at: string } & T : never;

/** One transcript event: when it happened, and what happened. */
export type HarnessEvent = Timestamped<EventBody>;

/** RFC 3339 with milliseconds, UTC — the same format Rust writes. */
export function now(): string {
  return new Date().toISOString();
}

/** Timeline helpers keep every mapping site short and identical. */
export function status(state: SessionState): HarnessEvent {
  return { at: now(), kind: "status", state };
}

export function delta(text: string): HarnessEvent {
  return { at: now(), kind: "delta", text };
}

export function message(role: Role, blocks: Block[]): HarnessEvent {
  return { at: now(), kind: "message", role, blocks };
}

export function toolCall(callId: string, name: string, title: string): HarnessEvent {
  return { at: now(), kind: "tool_call", call_id: callId, name, title };
}

export function toolResult(callId: string, toolStatus: ToolStatus, summary: string): HarnessEvent {
  return { at: now(), kind: "tool_result", call_id: callId, status: toolStatus, summary };
}

export function usage(input: number, output: number): HarnessEvent {
  return { at: now(), kind: "usage", input_tokens: input, output_tokens: output };
}

export function errorEvent(text: string, retryable: boolean): HarnessEvent {
  return { at: now(), kind: "error", message: text, retryable };
}

export function sessionStarted(harness: string, model: string | null, resumed: boolean): HarnessEvent {
  return { at: now(), kind: "session_started", harness, model, resumed };
}
