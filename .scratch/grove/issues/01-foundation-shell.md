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

**Status:** done

- [x] Rust workspace with `crates/grove-core` (no `tauri` dependency) and `apps/desktop/src-tauri`;
      npm workspace with `apps/desktop`, `packages/contract`, `sidecar`
- [x] `npm ci && npm run typecheck && npm test && npm run build` succeed from a clean checkout;
      `cargo test -p grove-core` and `cargo check -p grove-desktop` succeed on Windows
- [x] The shell renders the seven-panel sidebar, mounts only the active panel, and shows the version and
      platform returned by `app_info` through the bridge
- [x] `packages/contract` exports zod schemas + inferred types for `AppInfo`, `IpcError`,
      `EventEnvelope`, plus a fixture validator; `contract/fixtures/*.json` is asserted by both a Rust
      serde test and a TS zod test
- [x] Panels reach the core only through `src/ipc/bridge.ts`; tests inject `FakeBridge`, never Tauri
- [x] A test proves a `grove://session-event` envelope reaches a subscriber and that an unknown event
      kind or a malformed payload is ignored, not thrown
- [x] `.gitignore` covers `node_modules/`, `target/`, `dist/`, `.grove/`; README states prerequisites and
      the dev/build/test commands
- [x] `.github/workflows/ci.yml` runs on `windows-latest`: install, typecheck, vitest, vite build,
      rustfmt, clippy, core tests, desktop `cargo check`
- [x] Each of the seven panels has its own directory and component that later tickets replace; the
      registry is the only shared list
- [x] Styling tokens in `styles/tokens.css`; dark-first; no UI kit dependency

## Resolution

Shipped in `feat(shell): add Tauri 2 foundation shell, typed IPC bridge, and contract tests`.

Decisions worth knowing:

- The contract package is consumed as source through a Vite/tsconfig alias, so there is no build-order
  coupling between workspaces. Domain schemas live in their own modules and are re-exported from
  `index.ts` — one line per feature, which is the seam parallel tickets use.
- The Tauri bridge imports `@tauri-apps/api` lazily inside its methods, so every UI test runs under
  `FakeBridge` without loading Tauri at all.
- Tauri's Windows build requires a resource icon even with bundling off; `scripts/make-icon.mjs`
  generates `icons/icon.ico` with no image dependency, so the binary stays reproducible.
- Planned Rust and npm dependencies were pinned up front (`requirements.md`-driven), so later tickets
  rarely touch a dependency list.

Not verified: launching the GUI and observing the painted window — this environment cannot display one.
The shell's render is covered by jsdom tests and the desktop crate by `cargo check`.
