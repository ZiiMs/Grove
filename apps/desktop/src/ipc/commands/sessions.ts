import type {
  CreateSessionRequest,
  Decision,
  ProviderSpec,
  Session,
  SessionDetail,
  SessionSummary,
} from "@grove/contract";

/**
 * Command surface, one interface per domain. Later tickets add their interface to the intersection in
 * `commands/index.ts` — one line each, so parallel merges stay trivial.
 */
export interface SessionsCommands {
  /** Every registered provider, for the session picker. */
  provider_list: { args: Record<string, never>; result: ProviderSpec[] };
  session_list: { args: Record<string, never>; result: SessionSummary[] };
  session_get: { args: { id: string }; result: SessionDetail };
  session_create: { args: { req: CreateSessionRequest }; result: SessionDetail };
  /** `null` is what a Rust `IpcResult<()>` serialises to. */
  session_send: { args: { id: string; text: string }; result: null };
  session_cancel: { args: { id: string }; result: null };
  /**
   * Answer a pending permission request. The adapter owns the protocol (Codex's approval vocabulary,
   * ACP's permission outcome); the resolution arrives as a `permission_resolved` event, which is what
   * removes the buttons — this command returns nothing to render.
   */
  session_respond_permission: {
    args: { id: string; requestId: string; decision: Decision };
    result: null;
  };
  session_close: { args: { id: string }; result: null };
  session_resume: { args: { id: string }; result: Session };
}
