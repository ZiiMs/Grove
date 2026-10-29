# 0004 - Auto session titles call a model outside the agent loop

- **Status:** accepted
- **Date:** 2026-10-29
- **Changes:** `settings.titleModel`, `AgentProvider::complete`, `session_generate_title`,
  `title_model_suggest`, migration 9 (`sessions.title_source`)

## Context

Grove's session path is deliberately network-only where it must be: a session is a provider call, and
nothing else on that path reaches the network. That rule keeps an offline start usable and makes a
transcript's behaviour predictable. The session runtime is also the wrong tool for naming a
conversation: `AgentProvider::start` opens a conversation with tools, history, permissions and a
resumable session, all of which a title does not need and must not have.

An optional session title starts as the first user message (`title_source = auto`). That placeholder
is often long, quoted prose — useful, but not a name. Making it shorter without a model produces a
truncated command rather than a summary of the conversation.

## Decision

A session title may be written by **one small model call outside the agent loop**.

- **One global title model**, `{ provider, model } | null` in Settings. It is not per provider and is
  never reused for summaries or other background work.
- **The suggestion is a default, not a setting.** `title_model_suggest` answers with the small model
  of the first configured provider in catalog order (Anthropic → Claude Haiku 4.5, OpenAI → GPT-5
  Mini, Google → Gemini 2.5 Flash, OpenCode Go → DeepSeek V4.1 Flash, OpenRouter → Gemini 2.5 Flash
  when its live list offers it, else its first model). While the setting is null, generation uses
  that suggestion. It is not written to the settings table until the user saves Settings.
- **A one-shot completion path on the provider contract.** `AgentProvider::complete` takes a model,
  a system instruction, one prompt and a token budget. It sends no tools, no conversation history and
  no transcript; each wire module implements its vendor's non-streaming body. `WireProvider`
  dispatches on the existing wire format, so no provider module learns about another.
- **Budget: one call per session.** The automatic trigger fires once per auto session per app run,
  on the first assistant reply, guarded by a `Set` in the shell and a per-session in-flight guard in
  the core. A failure is not retried automatically. The user can ask again from the session menu,
  which always overwrites — including a manual title — and is the only path allowed to do so.
- **Failure leaves the placeholder.** A provider error is returned unchanged, the row keeps
  `title_source = auto`, and the UI toasts the model and provider. There is no silent fallback to a
  different provider or model: a title the user did not ask for from a model they did not choose is
  worse than the first-message placeholder.
- **Offline fallback.** The offline/preview provider answers `"Offline session"` deterministically, so
  tests and previews exercise the whole trigger without a key, a network or a vendor.

The title request carries the first user message and the first assistant reply, each truncated to
2 000 characters, asks for a 3–6 word title in the conversation's language, sets `max_tokens: 64`, and
turns thinking off. The answer is sanitized (trimmed, unwrapped from quotes/backticks, a leading
`Title:` dropped, whitespace collapsed, trailing `.`/`:` removed, capped at 60 characters); an empty
result is a generation failure.

## Why this is an ADR

This is hard to reverse once users expect model-written titles: the settings key, the title source
and the trigger are all observable, and removing the path would leave every auto session on a
placeholder. It is surprising without context because Grove otherwise promises that a session is the
only thing that talks to a provider. And it is a real trade-off against the offline-first invariant:
a fresh install with a key now makes one extra call per new auto session, and a title can only be
model-written while the provider is reachable.

## Consequences

- The "no network call on a session path" rule becomes: **no network call on a session path except
  the documented one-shot title completion**. That exception is this ADR, not a precedent for other
  background work.
- A user with no configured provider gets no automatic names; the placeholder stays. The settings
  pane says so instead of failing a call.
- The title model is a single choice, so switching it affects every future auto session at once.
  Existing generated titles are not rewritten.
- The suggested OpenRouter model depends on the account's live list. The core has no model cache, so
  the settings suggestion (which receives the live list from the shell) and the generation path (which
  uses the catalog fallback) can differ for that one provider until the user saves an explicit choice.
- Titles are cosmetic: an API failure never changes a session's status, transcript or ability to run.

## Alternatives considered

- **Derive the title from the assistant message without a model.** Rejected: it produces truncated
  prose or the first line of an answer, not a name, and fails on conversations whose subject is stated
  indirectly.
- **Reuse the agent loop with a tiny prompt.** Rejected: it exposes tools and history to a request that
  needs neither, spends a turn on the session's conversation, and would pollute the transcript or the
  resume token.
- **Generate at session creation.** Rejected: a blank-title session has no material until the first
  exchange, and a title from the opening request alone often names the task rather than the work.
- **A per-provider title model.** Rejected for this slice: one global small model is cheaper to
  reason about, and the session's provider is already a different axis from who names it.
