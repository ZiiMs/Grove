# 15: Hardening and release readiness — five harnesses, all green

**What to build:** The finished first build: all five harnesses selectable, every adapter passing the same
conformance suite, restart-resume proven per adapter, sessions working with no network, a README that gets
a newcomer to a first session, and CI that fails loudly on any regression.

**Context pointers:** `architecture.md` (all), `AGENTS.md`, `README.md`, every earlier ticket.

**Blocked by:** 04, 05, 06, 07, 10, 11, 12, 13, 14

**Status:** done

- [x] All five harnesses appear in the picker with accurate capabilities and doctor state; the mock harness
      stays available for offline demos
- [x] The conformance suite runs against all six adapters (mock + five) with their fixtures in CI
- [x] A restart test per adapter: start, send, shut down, reopen, resume, transcript intact
- [x] Offline test: with no server reachable, sessions against the fake harnesses still run and the catalog
      panel serves its cache
- [x] README quickstart covers prerequisites, dev, build, test, and a manual smoke checklist (launch, run a
      session, install a skill, open a PR panel)
- [x] CI runs everything on Windows — install, typecheck, vitest, vite build, rustfmt, clippy, core tests,
      desktop `cargo check` — with actionable output
- [x] No placeholder panels remain; every panel is reachable and covered by at least one test
- [x] Performance notes recorded in README: lazy panel mounting, Monaco split into a lazy chunk (assert in
      build output), virtualized event list
- [x] `architecture.md` reconciled with what shipped (adapter set, sidecar/pi RPC decision, plan
      mirror read-time check, resume-token slot, migrations); an ADR added for any decision that changed
      during the build; `GLOSSARY.md` created only with real terms used by the code
- [x] `docs/adapters.md` matches the code: the detection rule for adapters with no default binary, the
      `permission` conformance scenario, and a sentence for harnesses hosted behind the sidecar boundary;
      add the ACP launch/mapping section (the module doc in `acp.rs` is written to be lifted)
- [x] `docs/sidecar-protocol.md` is either registered as a spec node (with the specgraph test's expected
      node set updated deliberately) or explicitly documented as a non-spec design doc — not left ambiguous
- [x] `cargo clippy --workspace -- -D warnings` is clean and no scaffolding `#[allow(dead_code)]` remains
- [x] All ticket files carry `Status: done` with a `## Resolution` note naming the merge commit and checks

## Resolution

Shipped on `ticket/15-hardening`, based on `159fafd chore(tickets): close ticket 14`. The parent owns the
integration merge commit; this worktree leaves every change uncommitted.

Decisions worth knowing:

- **Restart matrix.** `tests/adapters_restart.rs` runs the same path for all six adapters through
  `SessionManager` and the existing `tests/adapter_support/*` fixtures: create → send → close (records
  `exited`) → reopen the SQLite file → resume. It asserts the pre-close transcript prefix is byte-for-byte
  intact, the stored resume token survives, the adapter's second `SessionStarted` has `resumed: true`, the
  resumed conversation keeps the same token, and a second turn leaves the prefix untouched. The OpenCode
  fixture keeps its `--state-dir` session store in a tempdir held for the test, so spawn-mode resume is
  real rather than a fresh conversation. The same file pins the picker table (ids + every capability flag)
  and that the doctor reports the same harnesses.
- **Offline proof.** `tests/offline.rs` is its own test binary because environment variables are
  process-global. It warms the skill catalog against a wiremock GitHub, then sets `HTTP(S)_PROXY`,
  `ALL_PROXY` and their lowercase forms to `http://127.0.0.1:9` and `NO_PROXY` to empty, creates a fresh
  skill service (so its client is built under the hostile environment) with a zero TTL, and asserts the
  catalog answers from cache with `offline: true`. In the same environment it runs one turn through
  `SessionManager` for each of the six fake harnesses; OpenCode's loopback client sets `no_proxy()`, so
  the HTTP/SSE path cannot be rerouted. The existing `skills_catalog::offline_catalog_serves_the_disk_cache`
  stays as the plain dead-port case.
- **Bundle guard.** `scripts/check-bundle.mjs` reads the built `index.html`, finds the entry chunk, and
  fails if it contains `monaco-editor` or if no lazy chunk does. It is the last step of `npm run build`
  and is also available as `npm run check:bundle`. It was verified to fail against a deliberately
  poisoned entry chunk and to pass on the real build.
- **No scaffolding.** Removed the unused `Placeholder` panel component and its CSS; every panel is now a
  real component with a test. Removed every blanket `#![allow(dead_code)]` from the integration-test
  support modules by making the support modules `pub` (their helpers are part of each test crate's public
  surface), and added `Default for Sandbox` rather than an allow. No production `allow(dead_code)` remains.
- **Docs.** `architecture.md` now records the six-adapter set (mock included), the four migrations
  (sessions/events, plans, settings, pins), the JSON `InstallStore` reserve, the sidecar protocol as a
  spec node, the bundle guard, and the restart/offline suites. `docs/adapters.md` gained §Detection (the
  no-default-binary rule and ACP's `unavailable`), the opt-in `permission` scenario, non-CLI fixtures
  (OpenCode HTTP/SSE, pi sidecar), the ACP launch/mapping section lifted from `acp.rs`, and the concrete
  resume-token forms. `docs/sidecar-protocol.md` gained spec frontmatter as `grove-sidecar-protocol`
  (parent `grove-architecture`); `tests/specgraph.rs` was deliberately updated for the new node/subtree
  edges. `docs/adr/0001-harness-protocols-and-sidecar.md` records Codex v2, Claude process-per-turn, pi
  RPC via sidecar, plan hash check instead of a watcher, context-as-snapshot, and opt-in permission
  conformance. `GLOSSARY.md` defines only harness, adapter, session, spec graph, plan mirror, sidecar and
  context document.
- **README/CI.** README has a quickstart to a first session, the full manual smoke checklist, and the
  performance notes (lazy panel mount, virtualized event list, Monaco lazy chunk, bundle guard, no session
  polling). CI keeps the same Windows gate set; the build step now names the bundle guard. The two
  `clippy::needless_question_mark` findings in `apps/desktop/src-tauri/src/commands/sessions.rs` were
  fixed, and `tests/adapters_pi.rs`'s missing-launcher test now passes an explicit fixture entry point so
  it no longer depends on `npm run build` having run first.
- **Dependencies.** No manifest dependency changed. The root `package.json` gained the `check:bundle`
  script and the build hook.

Checks (run in this worktree):

| Command | Result |
|---|---|
| `cargo test -p grove-core` | all targets pass, including `adapters_restart` (7) and `offline` (1) |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo fmt --all -- --check` | clean (after `cargo fmt --all`) |
| `cargo check -p grove-desktop` | clean |
| `npm run typecheck` | clean (desktop, contract, sidecar) |
| `npm test` | 11 desktop files / 63 tests, 8 contract files / 57 tests, 4 sidecar files / 31 tests |
| `npm run build` | vite + sidecar build + bundle guard pass |
| poison check for `scripts/check-bundle.mjs` | exits 1 when `monaco-editor` is appended to the entry chunk |

Not verified: launching the GUI and observing webview paint (this environment cannot display one); the
manual smoke checklist in the README is the human gate. Real harness binaries were never invoked — the
fixtures are the oracle, as with every adapter ticket.
