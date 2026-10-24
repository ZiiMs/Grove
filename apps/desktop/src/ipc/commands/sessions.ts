import type {
  CreateSessionRequest,
  HarnessInfo,
  Session,
  SessionDetail,
  SessionSummary,
} from "@grove/contract";

/**
 * Command surface, one interface per domain. Later tickets add their interface to the intersection in
 * `commands/index.ts` — one line each, so parallel merges stay trivial.
 */
export interface SessionsCommands {
  /** Every registered harness, for the session picker. */
  harness_list: { args: Record<string, never>; result: HarnessInfo[] };
  session_list: { args: Record<string, never>; result: SessionSummary[] };
  session_get: { args: { id: string }; result: SessionDetail };
  session_create: { args: { req: CreateSessionRequest }; result: SessionDetail };
  /** `null` is what a Rust `IpcResult<()>` serialises to. */
  session_send: { args: { id: string; text: string }; result: null };
  session_cancel: { args: { id: string }; result: null };
  session_close: { args: { id: string }; result: null };
  session_resume: { args: { id: string }; result: Session };
}
