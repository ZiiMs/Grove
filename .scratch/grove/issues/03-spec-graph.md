# 03: Spec graph — index, render, walk, edit

**What to build:** Grove reads the project's spec files, shows them as a navigable tree, renders a spec
readably beside a session, lets the user edit one in place, and refuses a save that would clobber a
change made elsewhere. A user opens the Specs panel, clicks from a goal spec to the module specs it
links to, and edits a body.

**Context pointers:** `architecture.md` §Subsystems (specgraph), §IPC contract, §Rendering. This repo's
own `goal-and-requirements.md` and `architecture.md` are valid spec nodes — use them as a fixture.

**Blocked by:** 01 (foundation shell)

**Status:** ready-for-agent

- [ ] Index finds `.md` files whose frontmatter has `id` + `type`; parses `parent`, `depends-on`,
      `references`, `implements`, `covers`, `title`, `status`
- [ ] Graph queries: tree/roots, ancestors, subtree, neighbours; validation reports dangling links,
      duplicate ids, and parent cycles with the offending paths
- [ ] `spec_read` returns frontmatter, body, and content hash; `spec_write(expected_hash)` succeeds once
      and then returns `conflict` until the file is re-read
- [ ] A file watcher emits `grove://spec-changed` (coalesced) when specs change on disk
- [ ] Specs panel: tree grouped by type with status, readable render using `markdown-it` with HTML
      disabled, Monaco editor lazy-loaded only when editing, conflict surfaced with a reload affordance
- [ ] Tests run against a temporary fixture repo with a known graph (dangling link, cycle, duplicate id);
      contract fixtures for `SpecGraph`, `SpecNode`, `SpecDoc`, `SpecValidation`
- [ ] UI tests cover open, save, and conflict
- [ ] Indexing is bounded and skips `node_modules`, `target`, `.git`, `dist`
