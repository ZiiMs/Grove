# 0003 - Providers replace harnesses: Grove runs the agent itself

- **Status:** accepted
- **Date:** 2026-10-25
- **Supersedes:** ADR 0001 (harness protocols and the sidecar boundary)
- **Changes:** `architecture.md` — the provider contract, the agent runtime, migration 6

## Context

The first build wrapped other people's agents. A session launched a CLI (Claude Code, Codex,
opencode, `pi`, an ACP agent) and translated its output into normalized events. That bought real
capability on day one, and cost three things that turned out to matter more:

1. **An install prerequisite.** Nothing worked until the user had a specific CLI on `PATH`, and
   "harness not detected" was the first failure most people hit.
2. **A protocol per harness.** Five wire formats, each versioned by someone else, each with its own
   fixture suite. `docs/adapters.md` existed to explain how to survive the next one.
3. **Capability drift.** The adapter could translate what the CLI printed, but it could not decide how
   a turn runs, so the tool loop, approvals and cancellation semantics differed per harness.

The product intent is one interface over models, not one interface over other products' CLIs.

## Decision

Grove runs the agent. A session picks a **provider**, a **model** and an **API key**; the runtime asks
the vendor for a turn, streams the text, and executes tools locally.

- **Providers:** Anthropic, OpenAI, Google and OpenRouter. Anthropic, OpenAI and Google each own a
  wire client; OpenRouter reuses the OpenAI one and differs only by catalog entry.
- **Auth:** API keys, stored in the OS keychain under `provider.<id>.api_key`, with the vendor's own
  environment variable as a fallback. There is no sign-in flow — see below.
- **Runtime:** one agent loop (`providers/agent.rs`) with a bounded tool loop, six tools, and
  approval gates on the three mutating ones. The normalized event model, pump, store and transcript
  are unchanged: providers replace the thing that produced events, not the event model.
- **Removed:** `adapters/` (six adapters), `harness_doctor.rs`, the Node `sidecar` workspace,
  `fixtures/harnesses/`, `docs/adapters.md`, `docs/sidecar-protocol.md`, and the adapter-specific
  integration suites.
- **Persistence:** migration 6 renames `sessions.harness` to `sessions.provider`. A row naming a
  retired harness is simply a provider this build does not register, which the picker already handles.
- **Skills:** with no per-vendor skill directories left, the two targets are the user's
  `~/.grove/skills` and the project's `.agents/skills`.

## Why API keys and not "sign in with your account"

Account sign-in was part of the original request, so it was checked per vendor before building it:

| Vendor | Account sign-in for third-party apps |
|---|---|
| Anthropic | Claude subscription OAuth is reserved for Anthropic's own clients; using it elsewhere violates the terms |
| OpenAI | ChatGPT OAuth is reserved for OpenAI's own clients |
| Google | Gemini subscription auth is bound to Google's first-party surfaces; API keys come from AI Studio |
| OpenRouter | **Supported and recommended**: "Sign In with OpenRouter" is an OAuth PKCE flow for third-party apps, no client registration required |

So three of the four have no legitimate sign-in, and the fourth does. This build ships API keys for all
four, because one key path that works everywhere beats three of them plus one flow that needs a browser
round-trip Grove cannot verify in CI.

**Not implemented:** the OpenRouter PKCE flow. `AuthMethod` carries `SignIn` and `ProviderSpec::auth`
already drives the settings row, so a provider that offers sign-in does not need a UI change to be
recognised — but the pane only renders the key form today, and the catalog marks OpenRouter `ApiKey`.
Making it a sign-in provider means: a loopback callback (RFC 8252), the code exchange against
`/api/v1/auth/keys`, and storing the returned key as the credential. That is the next step if a user
wants sign-in; until then an OpenRouter key from `openrouter.ai/keys` works exactly like the others.

## Consequences

- **No install step.** The only prerequisite is a key, and the settings pane says which page issues it.
- **Grove owns the turn.** Tool approvals, cancellation, the step ceiling and the transcript shape are
  Grove's decisions, identical across vendors. That is what makes the UI honest.
- **Grove owns the failures.** A vendor change is now our protocol work rather than theirs. The
  mitigation is the same rule the adapters had: unknown stream frames are dropped, never fatal, and
  every parse error becomes a transcript row.
- **Cost moves to the user's key.** There is no subscription to piggyback on, so the settings pane
  links the vendor's own usage page instead of pretending to meter it.
- **The offline provider is not a fallback product.** `providers/offline.rs` keeps previews and tests
  runnable without a key; it is absent from the shipped catalog so nobody mistakes it for a model.
