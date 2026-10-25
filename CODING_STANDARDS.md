# Coding standards

Read at **review**, not while implementing. Each rule names the file that owns it, so the reviewer
checks the source rather than this summary; this file only says what counts as a violation.

## Authoritative sources

| For | Read |
|---|---|
| Shape, seams, contracts | `architecture.md` (especially §The provider contract, §IPC contract, §Extension seams) |
| Domain vocabulary | `GLOSSARY.md` |
| Why a decision changed | `docs/adr/` |
| Tracker and triage conventions | `docs/agents/` |

## Hard violations

A diff that breaks one of these is rejected, not discussed. Tooling enforces some of them; the rest
are the reviewer's job.

- **Provider isolation.** A module under `providers/` imports no other provider, no `tauri`, and no UI
  code. `crates/grove-core` never depends on `tauri`.
- **Rust owns the wire shape.** A DTO change lands with its `contract/fixtures/*.json` update and the
  matching zod schema in `packages/contract`. A new DTO without a fixture is incomplete.
- **Migrations are append-only.** `MIGRATIONS[..5]` is frozen in
  `crates/grove-core/tests/fixtures/schema_v5.sql`; editing shipped DDL fails a test. Add migration
  N+1 and bump the frozen slice.
- **Persisted payloads stay readable.** Renaming a field stored in `events.payload` needs a
  `#[serde(alias = …)]`; an old transcript that stops loading is a violation.
- **Secrets never leave the keychain.** `settings`, fixtures, logs and DTOs carry key *names* only.
  There is no command that returns a secret value.
- **Tool paths are sandboxed.** Anything the agent may touch resolves inside the session's working
  directory, including through a link.
- **One line per shared seam.** `lib.rs` fields, the `invoke_handler!` list, the provider catalog,
  `App.tsx`, and the dependency files get one added line, so parallel work merges.

## Judgement calls

No check can settle these; they are the reviewer's opinion and should be stated as such.

- **Names reveal behaviour.** `decoder`, `probe`, `pump`, `catalog` are the vocabulary; a new word
  needs to earn its place.
- **One shape, one place.** A logic shape copied into a second hunk gets extracted, even when the two
  copies differ by a constant.
- **A module changes for one reason.** A file edited for three unrelated features is split before it
  grows.
- **Evidence over adjectives.** A comment explains a decision or a hazard; it does not restate the
  code.

## What review does not do

Rerun the gates by hand — CI runs them (`npm run typecheck`, `npm test`, `npm run build`,
`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`). Review spends its budget on what the
gates cannot see.
