# Grove — agent guide

Grove is a Tauri 2 desktop app (Rust core + React/TS webview) that runs AI coding agents against model
providers behind one interface and keeps the project's specs, skill catalog, TODO plan, and git/PR
state in view.

Read `architecture.md` before changing anything structural; it is the authoritative design and names
the extension seams. `goal-and-requirements.md` is the intent of record.

## Agent skills

### Issue tracker

Work is tracked as ClickUp tasks in the `Grove` folder's `Tickets` list; specs stay in the repo.
See `docs/agents/issue-tracker.md`.

A feature-sized request gets a ticket before code is written. The ticket is the spec the review stage
reads, so a change reviewed only against a chat message has no spec axis at all. A ticket goes to
`in progress` and to you when work starts, and gets a summary comment plus `complete` when it lands.

### Review

Standards live in `CODING_STANDARDS.md`; it is read at review, not while implementing.

### Triage labels

The five triage state roles plus `bug`/`enhancement`, applied as ClickUp space tags. See
`docs/agents/triage-labels.md`.

### Domain docs

Single-context: one root `GLOSSARY.md` and ADRs under `docs/adr/`. See `docs/agents/domain.md`.

## Build and verify

```
npm ci                 # workspace install (apps/desktop, packages/contract)
npm run typecheck      # tsc --noEmit across workspaces
npm run check:rust     # fast incremental core check for the edit loop (bypasses sccache)
npm test               # vitest (UI, contract)
npm run build          # vite production build + the entry-size bundle guard
cargo nextest run -p grove-core
cargo test -p grove-core --doc   # nextest does not run doctests
cargo check -p grove-desktop
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings
```

Rust builds go through sccache (`.cargo/config.toml`) and tests through nextest; both must be on
`PATH`. `cargo nextest run` does not run doctests, so run the separate `--doc` command when a crate
adds one.

Never add a network call to a session path. Only the skill catalog, the live provider model refresh,
and the cached public model-capability catalogue fetch; all three have a cache or static fallback.
Secrets go through `SecretStore` (OS keychain) — never into `settings`, fixtures, or logs.

## Conventions

- One module per vendor wire format, isolated behind `providers::AgentProvider`; no provider module
  imports another provider, `tauri`, or UI code.
- Rust owns the wire shape; TS validates with zod from `packages/contract`. Add a contract fixture when
  you add a DTO.
- Keep shared-file edits to one line per seam (see `architecture.md` §Extension seams) so parallel
  tickets merge cheaply.
