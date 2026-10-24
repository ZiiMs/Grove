# 15: Hardening and release readiness — five harnesses, all green

**What to build:** The finished first build: all five harnesses selectable, every adapter passing the same
conformance suite, restart-resume proven per adapter, sessions working with no network, a README that gets
a newcomer to a first session, and CI that fails loudly on any regression.

**Context pointers:** `architecture.md` (all), `AGENTS.md`, `README.md`, every earlier ticket.

**Blocked by:** 04, 05, 06, 07, 10, 11, 12, 13, 14

**Status:** ready-for-agent

- [ ] All five harnesses appear in the picker with accurate capabilities and doctor state; the mock harness
      stays available for offline demos
- [ ] The conformance suite runs against all six adapters (mock + five) with their fixtures in CI
- [ ] A restart test per adapter: start, send, shut down, reopen, resume, transcript intact
- [ ] Offline test: with no server reachable, sessions against the fake harnesses still run and the catalog
      panel serves its cache
- [ ] README quickstart covers prerequisites, dev, build, test, and a manual smoke checklist (launch, run a
      session, install a skill, open a PR panel)
- [ ] CI runs everything on Windows — install, typecheck, vitest, vite build, rustfmt, clippy, core tests,
      desktop `cargo check` — with actionable output
- [ ] No placeholder panels remain; every panel is reachable and covered by at least one test
- [ ] Performance notes recorded in README: lazy panel mounting, Monaco split into a lazy chunk (assert in
      build output), virtualized event list
- [ ] `architecture.md` reconciled with what shipped; an ADR added for any decision that changed during the
      build; `GLOSSARY.md` created only with real terms used by the code
- [ ] `cargo clippy --workspace -- -D warnings` is clean and no scaffolding `#[allow(dead_code)]` remains
- [ ] All ticket files carry `Status: done` with a `## Resolution` note naming the merge commit and checks
