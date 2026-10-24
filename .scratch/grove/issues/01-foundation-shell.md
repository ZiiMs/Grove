# 01: Foundation shell — Tauri 2 app, panel registry, typed IPC, contract tests, CI

**What to build:** A launchable Grove desktop window with the seven-panel shell and exactly one typed
command plus one typed event flowing end to end, on top of the test and CI harness every later ticket
extends. A user opens the app, sees Sessions · Skills · Specs · Plan · MCP · Git · Settings, switches
panels, and the app reports its version and data paths.

This ticket creates the seams listed in architecture.md §Extension seams, so later tickets only add a
line each.

**Context pointers:** `architecture.md` §Repository layout, §IPC contract, §Rendering, §Testing,
§Extension seams. `AGENTS.md` for the commands that must work at the end.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] Rust workspace with `crates/grove-core` (no `tauri` dependency) and `apps/desktop/src-tauri`;
      npm workspace with `apps/desktop`, `packages/contract`, `sidecar`
- [ ] `npm ci && npm run typecheck && npm test && npm run build` succeed from a clean checkout;
      `cargo test -p grove-core` and `cargo check -p grove-desktop` succeed on Windows
- [ ] Launching shows the seven-panel sidebar; only the active panel mounts; `app_info` returns version,
      app-data dir, and repo paths through the bridge
- [ ] `packages/contract` exports zod schemas + inferred types for `AppInfo`, `IpcError`,
      `EventEnvelope`, plus a fixture validator; `contract/fixtures/app-info.json` is asserted by both a
      Rust serde test and a TS zod test
- [ ] Panels reach the core only through `src/ipc/bridge.ts`; tests inject `FakeBridge`, never Tauri
- [ ] A test proves a `grove://session-event` envelope reaches a store and that an unknown event kind
      is ignored, not thrown
- [ ] `.gitignore` covers `node_modules/`, `target/`, `dist/`, `.grove/`; README states prerequisites and
      the dev/build/test commands
- [ ] `.github/workflows/ci.yml` runs on `windows-latest`: install, typecheck, vitest, vite build,
      rustfmt, clippy, core tests, desktop `cargo check`
- [ ] Each of the seven panels has its own directory and component that later tickets replace; the
      registry is the only shared list
- [ ] Styling tokens in `styles/tokens.css`; dark-first; no UI kit dependency
