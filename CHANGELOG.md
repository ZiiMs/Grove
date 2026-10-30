# Changelog

All notable changes to Grove are recorded here. Versions follow SemVer and
entries are generated from conventional commits by `scripts/release.mjs`.

## v0.3.0 — 2026-10-30

### Added

- **app:** use the mint robot garden as the app icon
- **app:** add the three-flower mark as app and tray icons
- **worktree:** name worktrees independently of session titles
- **theme:** default a fresh install to Signal
- **sidebar:** make the archive view unmistakable and date-grouped
- **sidebar:** toolbar, archive view, filters, sort, pins and recent
- **git:** archive a worktree's sessions when it is deleted
- **session:** show tool and file-type icons and hide instant timers
- **git:** delete a worktree with branch and force options
- **shell:** remember collapsed session-info sections
- **shell:** split this-session stats and make panel sections collapsible
- **usage:** show remaining OpenCode Go plan usage in the session panel
- **composer:** put approvals, model and thinking in the input footer
- **session:** stretch the transcript scrollbar and auto-hide it
- **shell:** keep the session info open by default
- **shell:** add a floating session info panel
- **composer:** move approvals beside the input and stack send over stop
- **shell:** drag the sidebar edge to resize it
- **picker:** filter folder children from the address bar
- **composer:** add approval mode and icon-only send controls
- **projects:** default a new project name from the folder
- **ui:** auto-name sessions and add the title model setting
- **core:** name sessions with a one-shot title model
- **desktop:** announce the queued model switch to assistive tech
- **ui:** make session titles optional and show first-message placeholders
- **core:** add session title source and first-message placeholders
- **desktop:** queue a cross-provider model switch until the turn is idle
- **sessions:** hand a live conversation to another provider without replaying it
- **providers:** export a session's normalized history for handoff
- **ui:** add the Archive section, archived pill and permanent delete
- **core:** archive, restore and permanently delete sessions
- **ui:** rename sessions inline from the tree
- **core:** rename a session with a validated title
- **desktop:** hot-swap same-provider models from the composer
- **sessions:** persist a live model change without a relaunch
- **providers:** hot-swap a session's model for the next turn
- **ui:** add the app-wide context menu
- **core:** reveal a path and remove a project
- render the inline thinking and model picker in the composer
- **desktop:** rebuild the folder picker with typed paths, drives and new folders
- **projects:** add folder roots, create-dir and a path normalizer
- **session:** cap the chat column and add a composer fade
- extend the shared combo with icons, disabled options and a footer
- add a model id label formatter
- **skills:** manage catalog sources and installed skills
- **agents:** add subagent workers with a session-tree view
- add searchable model combos and reposition toasts
- add custom window chrome and implicit session resume
- discover live provider models and add thinking levels
- add inline questions and airy session prose
- **providers:** add OpenCode Go
- **providers:** redesign the settings pane as compact cards
- **providers:** run the agent in-process against model providers
- **sessions:** pin specs into session context and switch harnesses
- **adapters:** add the Codex app-server adapter with fixture and conformance target
- **adapters:** add the OpenCode adapter over loopback HTTP and SSE
- **adapters:** add the pi adapter and the Grove Sidecar Protocol v1 sidecar
- **adapters:** add the generic ACP adapter over stdio JSON-RPC
- **adapters:** add the Claude Code adapter, conformance suite, and contributor guide
- **settings:** add typed settings, keychain secrets, and harness doctor
- **plan:** versioned per-session TODO plan with atomic agent mirror and Plan panel
- **mcp:** discover MCP servers and probe tools on demand
- **sessions:** add normalized event log, mock adapter, SQLite store, and sessions panel
- **git:** add gitops, the git panel, and the draft-PR flow
- **specs:** add spec graph index, validation, hash-guarded writes, and Specs panel
- **skills:** add cached GitHub catalog, preview, and atomic install
- **shell:** add Tauri 2 foundation shell, typed IPC bridge, and contract tests

### Changed

- **sidebar:** drop the archive count badge
- **shell:** remove the session info shadow
- **session:** let the chat header span over the info card
- **tree:** drop the model label and provider mark from session rows
- **shell:** pad the session info evenly in its column
- **shell:** centre the chat between the tree and the info card
- **shell:** inset the session info and hide its scrollbar
- **shell:** span the session info across the chat body
- **shell:** widen the project tree to 300px
- **dialog:** make every modal a translucent glass panel
- **ui:** move worker selection out of the App seam
- simplify thinking level labels
- bubble only user turns and footer assistant metadata
- apply rustfmt ordering to the adapter seams

### Fixed

- **session:** close archive, worker, and model-switch races
- **shell:** keep the session info floating below the header
- **window:** allow dragging the custom titlebar
- **ui:** keep status toasts up for four seconds
- **reveal:** hand Explorer native path separators
- close the session UX review findings
- **sessions:** name the conflict state and claim a queued switch before it launches
- **ui:** restore mocks between shell tests and use the mac redo shortcut
- **sessions:** clear awaiting on every stop path
- **skills:** clear stale install records when folders are gone
- **ui:** pass the transcript footer flag in the subagent view
- **agents:** scope provider tool ids to the session
- **thinking:** offer explicit none for zero-minimum budget models
- keep one assistant reply per turn in the transcript
- **build:** guard the entry bundle after the editor removal
- mark idle sessions exited on restart
- fold usage rows without duplicating assistant messages
- use configured providers and mark launched sessions ready
- **providers:** focus the field a dialog opens for
- resolve the two-axis review findings

### Docs

- record hot swap and context menu, refresh architecture
- document the title commands and setting
- record the auto session title ADR
- record the lossless cross-provider handoff as ADR 0005
- record archived sessions and migration 8
- add session UX polish implementation plan
- track work in ClickUp instead of local markdown tickets
- record that the gates are local now
- **tickets:** close ticket 08 and record the resume-token slot
- **tickets:** fold protocol research into the adapter tickets
- add Grove goal spec, architecture, tracker, and ticket graph

### Other

- **store:** prove an archived session survives a reopen
- **build:** run the JS toolchain under bun
- **contract:** populate the shared subagent summary fixture
- Fill the session column to the fullscreen width
- Stop empty context notices on model changes
- Serialize the xhigh thinking tier with its wire name
- Use model-specific thinking tiers from models.dev
- add a fast incremental Rust check
- **build:** cache Rust builds with sccache and test with nextest
- remove the GitHub Actions workflow
- guard migration history, normalise line endings, add review standards
- First init
- **tickets:** close ticket 14
- **tickets:** close ticket 10
- **tickets:** close ticket 12
- **tickets:** close ticket 13 and extend ticket 15
- **tickets:** close ticket 11
- **tickets:** close ticket 07
- **tickets:** close ticket 09
- Initial commit
