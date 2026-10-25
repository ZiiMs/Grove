# 03: Spec graph — index, render, walk, edit

**What to build:** Grove reads the project's spec files, shows them as a navigable tree, renders a spec
readably beside a session, lets the user edit one in place, and refuses a save that would clobber a
change made elsewhere. A user opens the Specs panel, clicks from a goal spec to the module specs it
links to, and edits a body.

**Context pointers:** `architecture.md` §Subsystems (specgraph), §IPC contract, §Rendering. This repo's
own `goal-and-requirements.md` and `architecture.md` are valid spec nodes — use them as a fixture.

**Blocked by:** 01 (foundation shell)

**Status:** done

- [x] Index finds `.md` files whose frontmatter has `id` + `type`; parses `parent`, `depends-on`,
      `references`, `implements`, `covers`, `title`, `status`
- [x] Graph queries: tree/roots, ancestors, subtree, neighbours; validation reports dangling links,
      duplicate ids, and parent cycles with the offending paths
- [x] `spec_read` returns frontmatter, body, and content hash; `spec_write(expected_hash)` succeeds once
      and then returns `conflict` until the file is re-read
- [x] A file watcher emits `grove://spec-changed` (coalesced) when specs change on disk
- [x] Specs panel: tree grouped by type with status, readable render using `markdown-it` with HTML
      disabled, Monaco editor lazy-loaded only when editing, conflict surfaced with a reload affordance
- [x] Tests run against a temporary fixture repo with a known graph (dangling link, cycle, duplicate id);
      contract fixtures for `SpecGraph`, `SpecNode`, `SpecDoc`, `SpecValidation`
- [x] UI tests cover open, save, and conflict
- [x] Indexing is bounded and skips `node_modules`, `target`, `.git`, `dist`

## Resolution

Shipped in `feat(specs): add spec graph index, validation, hash-guarded writes, and Specs panel`.

Decisions worth knowing:

- Frontmatter is parsed by a small line-based parser (`specgraph/frontmatter.rs`) rather than a YAML
  crate: the frontmatter dialect is flat key/values with inline or block lists, so a dependency plus a
  second error surface would not pay for itself. Quoted lists, CRLF, BOM and comments are covered by
  tests.
- The project root comes from `GroveCore::project_root` (`GROVE_PROJECT_ROOT`, else cwd), so commands
  need no extra argument and tests pin it with `with_project_root`.
- Indexing is bounded (5000 files, 50k entries, 2 MiB per spec) and skips `node_modules`, `target`,
  `.git`, `dist`; the repo itself is used as a real fixture, which keeps the graph honest.
- `SpecChange` was added beyond the four required DTOs because every new DTO gets a fixture (AGENTS.md).

Known limits: the GUI was not launched (no display here), real Monaco rendering is mocked in jsdom, and
Tauri's camelCase argument mapping is confirmed against `tauri-macros` rather than exercised at runtime.
