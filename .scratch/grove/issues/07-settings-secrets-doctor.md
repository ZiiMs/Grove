# 07: Settings, OS-keychain secrets, harness doctor

**What to build:** A user sets harness binary paths and catalog options, enters tokens that are stored in
the OS keychain and never displayed again, and sees which harnesses, `git`, `gh`, and Node are actually
available with versions — before starting a session rather than after it fails.

**Context pointers:** `architecture.md` §Subsystems (secrets, harness_doctor, settings), §Security.
Keychain is Windows Credential Manager via the `keyring` crate.

**Blocked by:** 01 (foundation shell)

**Status:** done

- [x] Settings store keeps JSON per key with schema validation and defaults for harness paths, catalog
      sources, and UI theme; unknown keys are preserved
- [x] `SecretStore` trait with a keychain implementation and an in-memory implementation for tests;
      commands expose set / status / delete only — there is no read-back of a value
- [x] A test asserts no secret value appears in the SQLite file bytes or in logs
- [x] Doctor resolves each harness binary (configured path, then `PATH`), runs `--version` with a
      timeout, and reports installed / version / path / reason; it also reports node, git, gh
- [x] A missing or failing harness never fails the app: non-zero exit becomes `installed: false` with a
      reason
- [x] Settings panel: path overrides, secret entry with a status chip (never the value), doctor table with
      refresh
- [x] The real-keychain path has one `#[ignore]`d integration test; regular tests use the memory store
- [x] Contract fixtures for `Settings` and `DoctorReport`

## Resolution

Merged as `feat(settings): add typed settings, keychain secrets, and harness doctor`.

Decisions worth knowing:

- `GroveCore::new` (tests/previews) keeps `MemorySecretStore`; `GroveCore::init` (the app) hands services
  the real `KeychainSecretStore`. Regular tests therefore never touch Windows Credential Manager; one
  `#[ignore]`d test exercises it and was run once (§verification in the branch).
- Migration numbering was reconciled at merge with ticket 09: `plans` is migration 2, `settings` is
  migration 3, and the applied-migrations assertions now expect `[1, 2, 3]`.
- `StoreError::Invalid` was added so settings validation maps to the wire's `invalid` code instead of `io`.
- The doctor asks the adapters themselves (`SessionManager::detect_all`), so a new harness shows up in the
  table without the doctor knowing its name.
- Saved catalog sources are applied to the live `SkillService` at startup and on save, so the Skills panel
  and settings cannot disagree.

Not verified: the keychain `unavailable` branch (a locked credential store cannot be produced
deterministically in a test); Tauri handler invocation and a GUI smoke run, as with every other panel.
