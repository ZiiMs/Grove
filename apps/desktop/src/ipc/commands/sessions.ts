import type {
  CreateSessionRequest,
  Decision,
  ProviderSpec,
  QuestionAnswer,
  Session,
  SessionDetail,
  SessionSummary,
  ThinkingLevel,
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
  /** Answer a pending structured question; `question_resolved` carries what the runtime accepted. */
  session_respond_question: {
    args: { id: string; requestId: string; answers: QuestionAnswer[] };
    result: null;
  };
  /** Change the reasoning level for the next turn; returns the refreshed session detail. */
  session_set_thinking: { args: { id: string; level: ThinkingLevel }; result: SessionDetail };
  /**
   * Change the model for the next turn in place. `thinking` is the nearest level the new model
   * supports, or `null` when the capability map has not loaded; the pair is persisted together.
   */
  session_set_model: {
    args: { id: string; model: string; thinking: ThinkingLevel | null };
    result: SessionDetail;
  };
  session_close: { args: { id: string }; result: null };
  session_resume: { args: { id: string }; result: Session };
}
