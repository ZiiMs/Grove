# 05: Skill catalog — browse, preview, install into a chosen harness directory

**What to build:** A user browses public GitHub skill catalogs in Grove, previews a skill, picks a
destination from a dropdown (each harness's own skills directory, or the project's `.agents/skills`), and
installs it. The installed skill is a normal folder the harness sees — no new format, no lock-in. The
whole flow works offline from the cache.

**Context pointers:** `architecture.md` §Subsystems (skills), §Windows-first, §Security (token in
keychain, never in settings).

**Blocked by:** 01 (foundation shell)

**Status:** ready-for-agent

- [ ] Sources are configurable GitHub repos with defaults; the manifest is fetched via the GitHub API,
      cached with TTL + ETag, and served from cache when the network is unavailable; a refresh command
      bypasses the cache
- [ ] Only directories containing `SKILL.md` become entries; preview shows frontmatter and body
- [ ] `skill_targets()` lists the per-harness directories (claude, codex, pi, opencode) plus the
      project-local `.agents/skills`, each marked existing/not
- [ ] Install copies a skill folder atomically into the chosen target, refuses `..`, absolute paths, and
      symlink escapes, records the install, and reports new vs overwritten
- [ ] An optional GitHub token is read through `SecretStore`; it is never logged and never written to
      settings or the DB
- [ ] Tests use a local mock HTTP server and tempdir targets; traversal, overwrite, and offline cases are
      covered
- [ ] Skills panel: search/filter, preview, destination dropdown, install with success/failure shown;
      panel stays usable with the network down
- [ ] Contract fixtures for `CatalogEntry`, `SkillTarget`, `InstallResult`
