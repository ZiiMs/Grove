import type { SessionContext, SessionDetail } from "@grove/contract";

/**
 * Session knowledge commands: what a session carries (pinned specs plus its plan), and the two ways
 * it moves — an explicit refresh and a provider switch.
 *
 * A pin is durable (`pins` in SQLite) and a snapshot is taken at start, so `session_context_get` reads
 * the current knowledge while `session_refresh_context` is the only thing that pushes new knowledge
 * into a *running* conversation.
 */
export interface SessionContextCommands {
  /** The session's pins and the documents a start would send right now. */
  session_context_get: { args: { id: string }; result: SessionContext };
  /** Pin (`pinned: true`) or unpin a spec; the refreshed snapshot comes back. */
  session_pin: { args: { id: string; specId: string; pinned: boolean }; result: SessionContext };
  /** Re-read pins + plan, send them to the running agent, and note it in the transcript. */
  session_refresh_context: { args: { id: string }; result: SessionContext };
  /** Re-launch the same conversation on another provider; the transcript is kept. */
  session_switch_provider: {
    args: { id: string; provider: string; model?: string };
    result: SessionDetail;
  };
}
