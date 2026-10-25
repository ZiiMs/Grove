# 05: Skill catalog — browse, preview, install into a chosen harness directory

**What to build:** A user browses public GitHub skill catalogs in Grove, previews a skill, picks a
destination from a dropdown (each harness's own skills directory, or the project's `.agents/skills`), and
installs it. The installed skill is a normal folder the harness sees — no new format, no lock-in. The
whole flow works offline from the cache.

**Context pointers:** `architecture.md` §Subsystems (skills), §Windows-first, §Security (token in
keychain, never in settings).

**Blocked by:** 01 (foundation shell)

**Status:** done

- [x] Sources are configurable GitHub repos with defaults; the manifest is fetched via the GitHub API,
      cached with TTL + ETag, and served from cache when the network is unavailable; a refresh command
      bypasses the cache
- [x] Only directories containing `SKILL.md` become entries; preview shows frontmatter and body
- [x] `skill_targets()` lists the per-harness directories (claude, codex, pi, opencode) plus the
      project-local `.agents/skills`, each marked existing/not
- [x] Install copies a skill folder atomically into the chosen target, refuses `..`, absolute paths, and
      symlink escapes, records the install, and reports new vs overwritten
- [x] An optional GitHub token is read through `SecretStore`; it is never logged and never written to
      settings or the DB
- [x] Tests use a local mock HTTP server and tempdir targets; traversal, overwrite, and offline cases are
      covered
- [x] Skills panel: search/filter, preview, destination dropdown, install with success/failure shown;
      panel stays usable with the network down
- [x] Contract fixtures for `CatalogEntry`, `SkillTarget`, `InstallResult`

## Resolution

Shipped in `feat(skills): add cached GitHub catalog, preview, and atomic install`.

Decisions worth knowing:

- `skills/catalog.rs` caches the recursive git tree per source under `AppPaths.cache/skills`, with a
  15-minute TTL and ETag revalidation (`If-None-Match` → 304 keeps the cache). A failed fetch with a
  cache present returns the cache and sets `offline`; `refresh` bypasses a fresh TTL but still falls back
  on failure.
- Blobs are cached by their immutable git SHA, so preview and install keep working offline once a skill
  has been seen. Install validates every tree entry (traversal, absolute paths, `mode 120000` symlinks)
  before downloading and writes through a staging directory renamed over the destination.
- Symlink escapes on the destination side are refused by canonicalising the target root and the existing
  destination (a Windows junction to outside is covered by a test).
- `InstallStore` is a trait; the shipped implementation is a JSON file under the app-data dir. When the
  SQLite store from ticket 02 lands it can replace this behind the trait, using the `installs` table
  shape from `architecture.md`.
- `SecretStore` lives in `secrets/mod.rs` with a redacting `MemorySecretStore`; ticket 07 swaps in the
  keychain. The token is read per request and never stored by the skills module; an app-data-wide byte
  scan asserts it never reaches disk.
- `skill_targets`/`skill_install` take an optional `projectRoot`; the UI passes none until the session
  work introduces a project, so `.agents/skills` resolves against the process cwd for now.
- The Tauri commands are thin adapters (`commands/skills.rs`); contract fixtures are asserted by
  `crates/grove-core/tests/skill_contract.rs` and `packages/contract/test/skills.test.ts`.
