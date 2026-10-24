# 07: Settings, OS-keychain secrets, harness doctor

**What to build:** A user sets harness binary paths and catalog options, enters tokens that are stored in
the OS keychain and never displayed again, and sees which harnesses, `git`, `gh`, and Node are actually
available with versions — before starting a session rather than after it fails.

**Context pointers:** `architecture.md` §Subsystems (secrets, harness_doctor, settings), §Security.
Keychain is Windows Credential Manager via the `keyring` crate.

**Blocked by:** 01 (foundation shell)

**Status:** ready-for-agent

- [ ] Settings store keeps JSON per key with schema validation and defaults for harness paths, catalog
      sources, and UI theme; unknown keys are preserved
- [ ] `SecretStore` trait with a keychain implementation and an in-memory implementation for tests;
      commands expose set / status / delete only — there is no read-back of a value
- [ ] A test asserts no secret value appears in the SQLite file bytes or in logs
- [ ] Doctor resolves each harness binary (configured path, then `PATH`), runs `--version` with a
      timeout, and reports installed / version / path / reason; it also reports node, git, gh
- [ ] A missing or failing harness never fails the app: non-zero exit becomes `installed: false` with a
      reason
- [ ] Settings panel: path overrides, secret entry with a status chip (never the value), doctor table with
      refresh
- [ ] The real-keychain path has one `#[ignore]`d integration test; regular tests use the memory store
- [ ] Contract fixtures for `Settings` and `DoctorReport`
