# 0005 - Lossless cross-provider model handoff

- **Status:** accepted
- **Date:** 2026-10-29
- **Changes:** `architecture.md` — the `AgentSession` contract, `StartContext`, the provider switch

## Context

Grove runs the agent in-process, and the wire clients are stateless per turn: every request carries
the whole conversation. The runtime already keeps that conversation in one neutral shape
(`wire::Message`: user, assistant and tool blocks), which is what the tool loop, the transcript and a
same-provider model hot swap use.

Changing **provider** was the exception. `session_switch_provider` stopped the old agent and sent the
new one a 24-line, text-only summary of the transcript. That is lossy in ways that matter:

1. **Tool calls and their results disappear.** The new model cannot see what was read, written or
   run, so it redoes work, re-reads files, or contradicts a decision the old model already made.
2. **Most of the conversation disappears.** Twenty-four lines is a tail, not a handoff, and long
   sessions lose the part where the plan and the constraints were agreed.
3. **The model is invited to restart.** A summary reads like an instruction to begin again, while the
   user asked for the same conversation on a different model.

A provider switch is a product feature in Grove ("use the model that fits this task"), not a request
for a fresh conversation. The runtime already has the exact conversation; the only reason it was not
handed over is that the provider contract had no way to export or seed it.

## Decision

Hand the normalized conversation to the new provider.

- **Export:** `AgentSession::history() -> Vec<wire::Message>` returns the running agent's
  conversation. The default returns empty, so a session (or a test stub) that keeps no in-process
  history keeps working.
- **Seed:** `StartContext.history: Vec<wire::Message>` carries it into the new agent.
  `WireProvider::start` seeds the agent's history from it when it is non-empty; otherwise it keeps the
  existing transcript-tail prompt (`StartContext.prompt`), which is still the resume path.
- **Switch:** on a live, idle session, `switch_provider` captures the history before stopping,
  persists the new provider and model, and launches with `history` and `prompt: None`. The transcript
  is untouched, and a real provider change still emits the visible provider-switch context note.
- **Trim:** the handoff ends at the last complete assistant message. An assistant message that asks
  for a tool is not complete — its results and the reply they produce are the rest of the turn — so a
  cancelled or failed turn never sends another provider the first half of a tool exchange.
- **Stop path:** a switch while the session is busy or waiting for a permission/question is refused
  with a typed `conflict` (`SessionError::Conflict` → `IpcError::conflict`). The shell queues the
  change, shows it in the composer, and retries when the session is idle. A stopped session has no
  live history left, so it keeps the transcript-replay path.

No new session event is added: the transcript is still the record, and the provider note plus the
header are the visible evidence of the change. No token or context-window accounting is added.

## Why not the alternatives

- **Keep the summary.** It is cheap, but it is exactly the loss this ADR exists to remove; a summary
  cannot carry a tool result the new provider would have to trust or re-derive.
- **Persist history in the store.** The transcript already persists the conversation, and the runtime
  history is the only copy where tool-call ids and arguments still line up with the messages that
  produced them. A second durable copy would need its own migration and could drift from the log.
- **Ask the vendor to continue the conversation.** Resume tokens name a conversation inside one
  vendor's API; no provider accepts another vendor's id. Cross-provider necessarily means replaying
  the conversation into the new wire format.

## Consequences

- **Provider isolation holds.** The handoff crosses the boundary as neutral `wire::Message` values;
  no provider module imports another and none learns a vendor's history format.
- **The contract grew.** `StartContext` and `AgentSession` are the module boundary every provider
  implements, so this is hard to reverse: removing the field means changing the trait, every stub and
  the switch path together. That is why it is an ADR.
- **A switch cannot interrupt a turn.** It is still stop-and-relaunch, so it is refused while busy or
  awaiting and queued by the shell instead. Waiting for a parked permission is deliberate: stopping
  would discard the request and strand the "needs you" badge.
- **No context-window guard.** A long conversation handed to a model with a smaller window can be
  rejected by the vendor on the next request. models.dev exposes per-model limits; a follow-up should
  warn or refuse before the handoff rather than discover it at the next turn.
- **Reasoning signatures do not survive.** Anthropic thinking blocks are already dropped by the
  decoder, so no switch — same provider or not — can claim to preserve them.
- **A dangling tool call is dropped, with the user message that opened it.** That is the deliberate
  trim rule; the alternative is sending a tool result to a provider that never asked for the call.
