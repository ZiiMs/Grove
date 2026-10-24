# 13: `pi` adapter + Node sidecar (Grove Sidecar Protocol v1)

**What to build:** A `pi` session in Grove, run through a Node sidecar because the `pi` SDK is JS-only. The
sidecar is a documented, swappable process boundary: any program speaking the protocol can drive a pi
session, and the Rust side never depends on the SDK.

**Context pointers:** `architecture.md` §Sidecar protocol, §The adapter contract. Ticket 08 owns the
conformance suite and fixtures.

**Blocked by:** 08 (adapter contract + Claude Code adapter)

**Status:** ready-for-agent

- [ ] `docs/sidecar-protocol.md` specifies Grove Sidecar Protocol v1 exactly as architecture.md describes:
      NDJSON JSON-RPC 2.0, `initialize`, `session.start/send/permission/cancel/stop`, and a
      `session.event` notification carrying a normalized `HarnessEvent`
- [ ] The sidecar implements that protocol; `pi_host.ts` dynamically imports the pi SDK behind an
      interface and reports `unavailable` with a reason when it is absent, so the sidecar runs without it
- [ ] The Rust `pi` adapter spawns the sidecar (config command, default `node sidecar/dist/main.js`),
      performs the handshake, forwards events, and maps handshake failure to `unavailable`
- [ ] Malformed sidecar output is logged and dropped; a dead sidecar yields `status exited` plus a
      retryable `error`
- [ ] The sidecar child is killed on shutdown (no orphans), verified by test; a resume token roundtrips
      through `session.start`
- [ ] Sidecar tests drive the RPC layer with a fake pi runtime; Rust tests use
      `fixtures/harnesses/pi_sidecar_fake.mjs`; the shared conformance suite passes
- [ ] The sidecar builds from the workspace (`npm run build`) and its dist is what the adapter launches
- [ ] Registered in `builtin_registry()`; doctor reports Node presence and whether the sidecar build exists
