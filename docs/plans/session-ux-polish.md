# Session UX polish — implementation plan

**Status:** draft for review · **Date:** 2026-10-29
**Tracker spec:** [Spec: Session UX polish](https://app.clickup.com/t/86e3x0z3q)

This is the implementation plan for five changes to the session surface. The ClickUp parent task is
the spec of record; this file keeps the file-level map, exact values, and verification matrix. No
tickets were created from this file automatically; the five ClickUp subtasks are the units of work.

## Scope

| # | Workstream | Size |
|---|---|---|
| A | Chat density and column width | S |
| B | Folder picker for adding projects | M |
| C | App-wide context menu, rename, archive, delete | L |
| D | Auto-generated session titles and the title model setting | M |
| E | Inline model/thinking picker and true model hot swap | L |

Recommended order: **A → B → C → D → E**. A and B are independent; C lands the shared menu primitive
that D's "Generate name with AI" action uses; E is the largest core change and benefits from the
session DTO work in C/D.

## Steps

- [ ] A. Chat density and 730px column
- [ ] B. Folder picker with typed paths, drives, new folder, blurred backdrop
- [ ] C. Context menu primitive, inline rename, archive section, permanent delete
- [ ] D. Optional titles, placeholder, global title model, auto/manual generation
- [ ] E. Inline model/thinking pair, provider glyphs, prettified labels, hot swap
- [ ] Contracts, fixtures, docs, ADRs, and the full verification matrix

---

## Settled decisions

### Composer picker

- Two separate click targets, side by side, not one combined chip:
  `[brain] High · [provider glyph] Sonnet 4.5`.
- Borderless text buttons. Hover adds a faint background; no chevron; no permanent pill or border.
- The thinking level is always visible, including `Off`.
- The thinking target opens a small level menu; the model target opens the grouped model menu.
- Model names are prettified: `claude-sonnet-4-5` → `Sonnet 4.5`; the exact id is shown as the
  option hint and in the trigger tooltip.
- Provider identity is a real monochrome vendor glyph tinted with the provider's `tint`; unknown
  providers fall back to the existing initials monogram.
- The model menu groups by provider, has search and a Recent group, and shows unconfigured providers
  disabled with a Settings hint. Picking a configured model switches provider in the same action.
- Same-provider model changes are hot: no relaunch, applied to the next turn.
- Cross-provider model changes are lossless: the live agent hands its full normalized history to the
  new agent. If a turn is running, the switch queues and lands at the next idle boundary; work is
  never cancelled to satisfy a model change.
- A model change never rewrites a manual title or interrupts an in-flight turn.

### Context menu

- One custom menu primitive used everywhere in the app; the native webview menu never appears.
- Text inputs and textareas get an edit menu (Undo, Redo, Cut, Copy, Paste, Select all).
- Session rows get a hover-only `…` menu button and a hover-only trash glyph that archives.
- Right-click works on sessions, projects, worktrees, messages, tool rows, tree background, and text
  fields.
- Archiving is reversible and needs no confirmation unless the session is running; permanent delete
  exists only inside Archive and asks for confirmation.

### Archive

- Any session, chat or project session, can be archived.
- Archive is a collapsible section pinned near the bottom of the sidebar, above the footer, with a
  count; it lists every archived session regardless of project.
- Archiving a running session prompts "Stop and archive", then stops the agent and hides the session.
- Archived sessions open normally, show an "Archived" pill and a Restore action, and sending a
  message auto-restores them with a toast.
- No automatic retention purge. Permanent delete is only in Archive; it removes the transcript and
  is guarded by a confirmation.

### Session titles

- The New Session dialog's title becomes optional. Blank means auto; a typed title is manual.
- An auto title shows a truncated first message immediately, then a model-generated title after the
  first assistant reply.
- Manual rename always wins and is never overwritten by auto-generation.
- A session menu item "Generate name with AI" can regenerate any title on demand, including a manual
  one, with no confirmation.
- The title model is one global small model, chosen in Settings → General; it is not per-provider.
- On first run, the effective default is the suggested small model of the first configured provider;
  once the user saves a choice it never changes silently.
- Generation failures show a toast naming the model and provider. There is no silent fallback to a
  different provider.

### Folder picker

- Browsing only: no `file://` URL handling is required.
- The picker has an always-editable address input, a clickable breadcrumb row, drive/root chips, and
  a folder list.
- Pasting `D:/Dev/grove` (or `D:\Dev\grove`) and pressing Enter navigates there.
- Clicking a folder row enters it. "Select folder" picks the current folder and returns to the
  project form with name and git detection prefilled.
- All mounted Windows drives are reachable; home and the filesystem root are always available.
- The dialog is ~760px wide over a blurred, dimmed app backdrop.
- "New folder" creates a directory in the current folder and navigates into it.

### Chat density

- Conversation and composer share a 730px maximum content width, centered.
- 16px between turns, 6px inside a turn, roughly 24px side padding.
- Prose line-height 1.72; user bubbles capped at about `min(82%, 560px)`.
- Composer pinned with a soft fade above it. No density setting.

---

## Domain terms to add when this lands

`GLOSSARY.md` should gain these entries with the implementation (not before):

- **Archived session** — a session whose `archived_at` is set. It leaves the active tree, appears in
  Archive, and can be restored; only archived sessions can be permanently deleted.
- **Title source** — `auto`, `generated`, or `manual`. Only `auto` is eligible for automatic naming;
  `manual` is never overwritten except by an explicit "Generate name with AI".
- **Title model** — the one global small model used to name sessions, stored in settings and resolved
  from a suggested default when unset.
- **Hot swap** — changing a live session's model without ending the conversation. Same-provider is
  in-place; cross-provider hands the normalized history to the new agent.
- **Context menu** — the single app-wide menu primitive that replaces the native webview menu.

---

# A. Chat density and column width

## Current state

`apps/desktop/src/styles/design.css` caps `.run-inner` and `.composer-inner` at 1080px, sets turn
spacing to `clamp(18px, 2.4vw, 22px)`, and uses `clamp(16px, 2.6vw, 34px)` horizontal padding. The
`.session` override block (around lines 1078–1113) wins over the responsive media queries, so the
mobile padding and bubble rules never apply to the session panel.

## Changes

1. Add a root token, `--chat-max: 730px`, and use it in both:
   - `.session .run-inner { max-width: min(var(--chat-max), 100%); }`
   - `.session .composer-inner { max-width: min(var(--chat-max), 100%); }`
2. Consolidate the session rules so there is one source of truth:
   - `.session .run-scroll { padding: 18px clamp(16px, 3vw, 24px) 26px; }`
   - `.session .composer { padding: 10px clamp(16px, 3vw, 24px) 14px; }`
   - `.session .turn { margin-bottom: 16px; }`
   - `.session .turn-foot { margin-top: 6px; }`
   - `.session .turn > p { font-size: 14px; line-height: 1.72; }`
   - `.session .turn-user .bubble { max-width: min(82%, 560px); }`
3. Delete the now-dead `.session` overrides inside the `max-width: 1279px`, `1099px`, `899px`, and
   `640px` media queries, and re-add only the rules that should actually change at those widths
   (padding at ≤899, full-width bubble at ≤640).
4. Add a composer fade: a `::before` gradient above `.composer` from transparent to `var(--bg)`, so
   the pinned composer reads as a surface without a hard line.
5. Keep `.run-rows` absolutely positioned; spacing stays a margin on `.turn`, not a flex gap.

## Acceptance

- At 1920px with the context panel open, the transcript and composer are 730px and aligned.
- No horizontal scrollbar appears from the narrower column; code blocks scroll internally.
- At ≤899px the session panel uses the full width with 14–16px padding.
- The composer stays pinned and the fade does not cover the last message.

## Tests

No unit tests; use the screenshot harness in `apps/desktop/tools/` and manual checks at 1920, 1440,
1280, 900, and 640.

---

# B. Folder picker

## Current state

`AddProjectDialog` shows the chosen folder as read-only text and a `Choose folder` button. Picker
mode (`fs_list_dirs`) starts at home and can only walk down; `parent` stops at a filesystem root.
There is no drive list, breadcrumb, typed path, or new-folder action. The dialog uses the plain
`dialog` backdrop, which has no blur.

## Core changes

### `fs_list_roots`

New command in `apps/desktop/src-tauri/src/commands/projects.rs`, backed by
`grove_core::projects::list_roots`.

```rust
pub struct FsRoot { pub name: String, pub path: String, pub kind: FsRootKind }
pub enum FsRootKind { Home, Drive, Filesystem }
pub struct FsRoots { pub home: Option<String>, pub roots: Vec<FsRoot> }
```

- Home: `home_dir()` from `projects/mod.rs`.
- Windows: probe `A:\` through `Z:\` with `metadata().is_dir()`; include those that exist. No new
  crate and no Win32 calls needed.
- Unix: include `/`; on macOS also include readable entries under `/Volumes` when present.
- Return display paths via `display_path` (forward slashes) so the UI compares strings consistently.

### `fs_create_dir`

New command `{ parent, name } -> DirEntry`.

- Validate `name` as a single path segment: trimmed, non-empty, no `/`, `\`, `..`, or control chars.
- Reject an existing path.
- Create with `std::fs::create_dir`; do not create recursively.
- Return the display path.

### Path normalization

`resolve_folder` already handles `~`, canonicalization, and Windows verbatim prefixes. Add a small
normalizer used by both commands and the UI:

- trim surrounding whitespace and quotes;
- convert `\` to `/`;
- collapse repeated separators;
- strip a trailing separator except at a root (`C:/`, `/`).

Do not add `file://` parsing.

### Contracts

Add `fs_roots` / `fs_create_dir` to `packages/contract/src/projects.ts`, `ipc/commands/projects.ts`,
and the command map. Add `contract/fixtures/fs-roots.json`; extend `dir-listing.json` if the shape
changes. Update `FakeBridge` handlers.

## UI changes

Rewrite the picker half of `apps/desktop/src/dialogs/AddProjectDialog.tsx`:

```
header:  title + close
address: [ Up ] [ editable path input ..................... ] [ New folder ]
         breadcrumb: Home / Dev / grove
chips:   [ ~ ] [ C: ] [ D: ] [ E: ]
list:    ../ (optional) + folder rows
footer:  [ Back ]                         [ Select folder ]
```

- **Address input:** always editable. Enter navigates; Escape reverts to the current path; invalid
  paths show an inline error under the bar. `data-autofocus` on open.
- **Breadcrumb:** clickable segments; the last segment is the current folder and is not a link. A
  click navigates directly.
- **Drive chips:** home + every root from `fs_list_roots`; the active root is highlighted.
- **List:** folders only, one row per directory; clicking enters; keyboard Up/Down moves selection,
  Enter enters, Backspace goes up. Keep the existing loading and empty states.
- **New folder:** toggles an inline name input under the address bar; Enter creates and navigates,
  Escape cancels; duplicate names show the error inline.
- **Select folder:** returns to the form with `path = listing.path`, then runs `project_detect` and
  auto-fills the name from `basename` only if the name is not dirty. This preserves the existing
  branch/remote form.
- Remember the last browsed path while the dialog stays mounted; reset on close.

## Styling

Add `className="add-project-dialog"` to the `Dialog` and a CSS block:

```css
dialog.add-project-dialog {
  width: min(760px, calc(100vw - 32px));
  background: color-mix(in srgb, var(--elev) 88%, transparent);
  backdrop-filter: blur(16px) saturate(1.1);
}
dialog.add-project-dialog::backdrop {
  background: color-mix(in srgb, var(--bg) 55%, #000);
  backdrop-filter: blur(14px) saturate(1.05);
}
```

The panel keeps the existing border/shadow/radius tokens. The folder list gets `max-height: 46dvh`
and its own scroll.

## Tests

- Rust: root enumeration is platform-shaped; `fs_create_dir` rejects separators, `..`, duplicates;
  normalizer handles `D:\Dev\grove\`, `D:/Dev/grove`, and `~`.
- UI: pasting a path + Enter loads the new listing; clicking a folder enters it; Select folder
  returns to the form and calls `project_detect`; New folder calls `fs_create_dir`; drive chips
  render from the fixture.
- Manual: add a project on `D:` from an app started in `C:`, with the blurred backdrop visible.

## Risks

- Drive probing is a handful of metadata calls on open; cache the roots for the dialog lifetime.
- Network/UNC paths are reachable by typing but are not enumerated as chips.
- Dot-prefixed folders remain hidden, matching current `list_dirs` behavior.

---

# C. App-wide context menu, rename, archive, delete

## C1. Menu primitive

New files:

- `apps/desktop/src/shell/ContextMenu.tsx` — types, host, item rendering.
- `apps/desktop/src/state/menuStore.ts` — `{ menu: MenuState | null, openMenu, closeMenu }`.

```ts
type MenuItem =
  | { kind: "item"; label: string; icon?: IconName; shortcut?: string;
      danger?: boolean; disabled?: boolean; onSelect: () => void }
  | { kind: "separator" };

type MenuState = { x: number; y: number; items: MenuItem[]; returnFocus: HTMLElement | null };
```

Behavior:

- Portal target: the topmost `dialog[open]` if one exists, otherwise `document.body`. This is
  required because a native `<dialog>` lives in the top layer and a body portal would render under
  it.
- Position `fixed` at the pointer, measure, flip/clamp inside the viewport, 8px margin.
- `role="menu"`, items `role="menuitem"`; Up/Down/Home/End move, Enter activates, Escape closes,
  Tab closes; focus returns to `returnFocus`.
- Close on outside pointerdown, scroll, resize, and window blur.
- Styling: `var(--elev)` surface, `var(--line-2)` border, `--r` radius, `var(--panel-shadow)`,
  28–30px rows, right-aligned shortcuts, `var(--red)` for destructive items.
- Global `contextmenu` listener in the host:
  - `preventDefault()` everywhere;
  - if the target is `input`, `textarea`, or `contenteditable`, open the edit menu;
  - if a component already called `preventDefault()` and opened its own menu, do nothing;
  - otherwise open the tree/background menu.

### Edit menu for text fields

Use `document.execCommand` for `undo`, `redo`, `cut`, `copy`, `paste`, `selectAll` first, because it
preserves the input's undo stack. Fall back to `navigator.clipboard.writeText` for copy and
`readText` + `setRangeText` for paste when `execCommand` is unavailable. If clipboard read is
denied, omit Paste rather than showing a dead item. Keyboard Ctrl/Cmd shortcuts keep working
unchanged.

### Surface registry

| Surface | Items |
|---|---|
| Session (active) | Rename, Generate name with AI, Archive, Copy working directory, Reveal in Explorer |
| Session (archived) | Restore, Generate name with AI, Copy working directory, Reveal in Explorer, Delete permanently |
| Project | New session, Copy path, Reveal in Explorer, Refresh, Remove project |
| Worktree | New session, Copy path, Reveal in Explorer, Refresh |
| Message | Copy, Copy as Markdown, Copy code (when the row contains a code block) |
| Tool row | Copy path, Reveal in Explorer (when the target resolves to a file/folder) |
| Text field | Undo, Redo, Cut, Copy, Paste, Select all |
| Tree background | New chat, Add project, Refresh |

New backend commands needed by the registry:

- `app_reveal_path(path)` — platform-specific, no shell interpolation. Windows: `explorer.exe` with
  `/select,` for a file, the folder for a directory. macOS: `open -R` / `open`. Linux: `xdg-open` on
  the parent or the directory.
- `project_remove(id)` — delete the projects row only. Sessions keep their rows and become chats
  (`project_id` is `ON DELETE SET NULL`); the working tree is untouched. Confirm in the UI.

### Hover actions

Add to `SessionRow` in `NavTree.tsx`:

- `…` button, `aria-label="Session actions"`, opens the session menu.
- Trash button, `aria-label="Archive session"`, title "Archive"; archives directly.
- Archived rows: restore icon button and `…`.

Buttons are opacity 0 until row hover or `:focus-within`; at `@media (hover: none)` they are always
visible. `stopPropagation` on click so selecting the row is not triggered.

## C2. Rename

- New core command `session_rename(id, title) -> SessionDetail` (or `Session`).
- Validate: trimmed, non-empty, no control characters, max 80 chars.
- Store update: `title`, `title_source = 'manual'`, `updated_at`.
- Append a `TitleChanged` event so every open view patches its title.
- UI: inline edit in the session row. Enter or blur commits, Escape cancels, empty cancels. The row
  becomes an `<input>` with the current title selected; do not open the session on click while
  editing.
- Archived rows are renameable too.

## C3. Archive

### Schema — migration 8

```sql
ALTER TABLE sessions ADD COLUMN archived_at TEXT;
CREATE INDEX sessions_by_archive ON sessions(archived_at, updated_at);
```

No backfill: `NULL` means active.

### DTO and contract

- `Session` gains `archivedAt: string | null`.
- `SessionSummary` and `SessionDetail` inherit it.
- Update `packages/contract/src/sessions.ts`, Rust `ipc/types.rs`, `contract/fixtures/session*.json`,
  and the fake bridge.

### Store and manager

- `set_session_archived(id, Option<&str>)` in `store/repos.rs`.
- `list_sessions` keeps returning all rows; the tree filters. Sort archived by `updated_at DESC`.
- `SessionManager::archive(id)`:
  - load the row;
  - if live, stop it cleanly and publish `Exited` so `awaiting` clears (see the regression note
    below);
  - set `archived_at = now`, return the session.
- `SessionManager::restore(id)`: clear `archived_at`, return the session.
- New commands `session_archive` / `session_restore`; add to the command map and fake bridge.
- The archive action never deletes events, plans, pins, or subagents.

### Tree UI

- Filter `sessions` into active and archived at the top of `NavTree`.
- Archive section after the projects, before `nav-foot`:
  - header with archive glyph, "Archive", count, caret;
  - rows reuse `SessionRow` with `archived` styling and the project name as a quiet suffix;
  - empty state "No archived sessions".
- Search: when the filter is non-empty, search both sections and keep the Archive header visible if
  it has matches.
- Restore-on-launch (`App.tsx`) must ignore archived sessions and only fall back to an active one.

### Opening and sending

- `SessionPane` shows an "Archived" pill in the header when `detail.archivedAt` is set, with a
  Restore button.
- `send` in the store: if `detail.archivedAt` is set, call `session_restore` first, update the store,
  toast "Session restored", then send.
- The composer stays enabled for archived sessions.

## C4. Permanent delete

- New command `session_delete(id)`.
- Core stops a live agent, then `DELETE FROM sessions WHERE id = ?`; `events`, `plans`, `pins`, and
  `subagents` cascade.
- UI: only the archived row menu exposes it; a `ConfirmDialog` names the session and says the
  transcript is deleted permanently. On success, remove the row from `sessions`, close its tab if
  open, and select the next active/archived row or clear the pane.
- No delete action anywhere on active sessions.

## C5. Regression to fix while here

`switch_provider`/`stop` currently call `shutdown()` without resolving permits, and the aborted pump
never publishes `Exited`, so a switch during a pending permission can leave `awaiting = 1` forever.
Archive and delete also stop live sessions. Add a test and ensure every stop path publishes `Exited`
(or explicitly clears `awaiting`) so the "needs you" badge cannot stick.

## Tests

- Menu: keyboard navigation, Escape, outside click, viewport clamping, menu above a native dialog,
  text-field edit actions, destructive styling.
- Rename: inline commit/cancel, empty title rejected, source becomes manual, event patches the tree.
- Archive: migration 8, archive hides from active tree, Archive section lists it, restore returns it,
  running archive stops first, send auto-restores.
- Delete: cascade rows gone, open tab closes, active selection moves.
- Tree: archived sessions are excluded from restore-on-launch and from the awaiting count.

---

# D. Auto-generated session titles

## D1. Schema and title source

### Migration 9

```sql
ALTER TABLE sessions ADD COLUMN title_source TEXT NOT NULL DEFAULT 'manual';
```

Values: `auto` (placeholder, eligible for generation), `generated` (model-written), `manual` (user-
written). Existing rows are manual, which preserves every title created by the current required
field. New sessions choose the source at creation.

`Session` DTO gains `titleSource: "auto" | "generated" | "manual"`. Update the contract, fixtures,
and fake bridge.

### New Session dialog

- Title becomes optional. Placeholder: "Auto from first message".
- Hint below: "Leave blank to name this session from its first message, then let a small model
  refine it."
- Blank creates with a temporary title ("New session") and `title_source = auto`; typed creates with
  `title_source = manual`.
- `CreateSessionRequest.title` becomes optional and the manager stops defaulting to
  `"<provider> session"` for the auto case.

### Placeholder on first send

In `SessionManager::send`, when `title_source = auto` and the title is still the temporary value:

- set the title to the first message, trimmed to 60 chars at a word boundary, with an ellipsis;
- keep `title_source = auto`;
- append `TitleChanged { title, source }` through the pump so the sidebar updates immediately.

## D2. One-shot title completion

The provider trait currently only starts an agent session, which would expose tools to the title
prompt. Add a narrow one-shot path instead.

### Trait and wire modules

```rust
pub struct CompletionRequest {
    pub model: String,
    pub system: String,
    pub prompt: String,
    pub max_tokens: u32,
}

async fn complete(&self, req: CompletionRequest, deps: ProviderDeps)
    -> Result<String, ProviderError>;
```

- Add it to `AgentProvider` in `providers/mod.rs` with a default `Unsupported` implementation so
  test stubs are not forced to implement it.
- Implement it on `WireProvider` by dispatching on `spec.wire` to a new non-streaming function in
  each vendor module: `anthropic::complete`, `openai::complete`, `google::complete`.
- Each function reuses that module's auth headers, endpoint, and body shape, sends
  `stream: false`, and extracts the first text block/choice/candidate. No tools, no history, no
  transcript.
- The offline provider returns a deterministic title (`"Offline session"`) for tests.

This keeps one wire format per module and adds no provider cross-imports.

### Title prompt and sanitizer

- System: "You name conversations. Reply with only a short title of 3–6 words, in the language of the
  conversation. No quotes, no punctuation at the end."
- User content: first user message + first assistant reply, each truncated to 2,000 chars.
- `max_tokens: 64`, thinking off.
- Sanitize: trim; strip surrounding quotes/backticks; drop a leading `Title:`; collapse whitespace;
  strip trailing `.`/`:`; cap 60 chars; reject empty.
- On empty/sanitize failure, treat it as a generation failure (toast), leaving the placeholder.

### Command

`session_generate_title(id) -> Session`

- Resolve the title model: explicit `titleModel` setting, else the suggested model of the first
  configured provider.
- Read the first user and first assistant text from the event log.
- Refuse with a clear `invalid` error when there is no title model or no assistant reply yet.
- Guard against concurrent generation for the same session with a `Mutex<HashSet<String>>` in the
  manager.
- On success: set title + `title_source = generated`, append `TitleChanged`, return the session.
- On failure: return the provider error unchanged; the UI toasts it. Keep the placeholder and
  `title_source = auto` so the user can retry or rename.

### Title model setting

Add a typed settings key rather than leaving it in `extra`:

```ts
titleModel: { provider: string; model: string } | null
```

- Rust `Settings` / `SettingsPatch` gain `title_model: Option<TitleModel>`; contract
  `settingsSchema` gains the optional object.
- `title_model_suggest` command returns the suggested model for the first configured provider:
  - anthropic → `claude-haiku-4-5`
  - openai → `gpt-5-mini`
  - google → `gemini-2.5-flash`
  - opencode-go → `deepseek-v4.1-flash`
  - openrouter → `google/gemini-2.5-flash` if present in its live list, else its first model
  - none configured → `null`
- The suggestion is the **effective** default when the setting is null; it is not silently persisted.
  Saving Settings persists whatever the form shows.

### Settings UI

In `GeneralPane`, add a "Session names" section before Permission mode:

- `Combo` "Title model", grouped by provider, with an "Off" option at the top.
- Default display: the effective suggestion, with a "Suggested" hint.
- Hint: "Runs after the first reply to name new sessions. One small call per session."
- No providers configured: disabled with "Add a provider key first."
- Include `titleModel` in the save payload; `workspaceStore` preferences gain the field.

## D3. Triggers

### Automatic

In the App session-event subscription, after `applyEvent`:

- when a `message` event with `role: "assistant"` arrives for a session whose `titleSource` is
  `auto`;
- and that session has not been attempted this app run (a `Set<string>` in the session store);
- call `session_generate_title` fire-and-forget.

The guard prevents duplicate calls. A failure toasts:

`Couldn't name this session with <Model Label> on <Provider>: <message>`

and leaves the placeholder. Manual regeneration remains available.

### Manual

- Session context menu: "Generate name with AI".
- Always enabled, including on manual titles and archived sessions.
- While a generation is in flight for that session, disable the item and show "Generating…" in the
  menu (or ignore a second invocation).
- Overwrites the title without confirmation; append `TitleChanged` with source `generated`.

### Sidebar update

Add `EventKind::TitleChanged { title, source }`:

- serialized kind `title_changed`;
- included in the contract `SessionEvent` union;
- `sessionStore.applyEvent` patches both the matching `sessions` row and `detail.title` when active;
- `NavTree` re-renders from the store.

## Tests

- Migration 9 defaults existing rows to manual.
- New session with a blank title creates `auto`; typed creates `manual`.
- First send writes the placeholder and keeps `auto`.
- First assistant message triggers one title call; a second message does not trigger another.
- Manual rename sets `manual`; auto-generation skips it.
- Manual "Generate name with AI" overwrites a manual title.
- Failure toast names the model/provider and keeps the placeholder.
- `title_model_suggest` picks the first configured provider and returns null when none are.
- Wiremock one-shot completion for Anthropic, OpenAI, and Google; no tools in the request body.
- Sanitizer tests: quotes, backticks, `Title:`, trailing punctuation, empty, 60-char cap.

## Risks / ADR

This adds a provider call outside the agent loop, which the repo's "no network on a session path"
rule currently forbids. Write **ADR 0004 — auto session titles call a model outside the agent loop**
with the exception, the one-call-per-session budget, the failure behavior, and the offline fallback
(leave the placeholder). It passes the ADR test: hard to reverse once titles are model-generated,
surprising without context, and a real trade-off against the offline-first invariant.

---

# E. Inline model/thinking picker and true hot swap

## E1. Presentation

### Provider glyphs

New `apps/desktop/src/workspace/ProviderGlyph.tsx`:

- Monochrome SVG paths for Anthropic, OpenAI, Google Gemini, OpenRouter, and OpenCode Go.
- `fill="currentColor"`, 14px default, tinted by `providerMeta(id).tint`.
- Unknown id: fall back to the existing `ProviderMark` monogram (`short`, tint).
- No new npm dependency; paths are inlined. Keep the attribution/source note in the file header.

### Model labels

New `apps/desktop/src/workspace/modelLabel.ts`:

- `modelLabel(model, provider)` returns the prettified name.
- Rules:
  - drop a gateway prefix that matches the provider (`anthropic/`, `openai/`, `google/`);
  - drop a leading vendor family token that the provider glyph already conveys
    (`claude-` for Anthropic);
  - strip a trailing date stamp (`-YYYYMMDD`) and `-latest`;
  - join digit groups (`4-5` → `4.5`, `2-5` → `2.5`);
  - title-case known tokens (`sonnet`, `opus`, `haiku`, `flash`, `pro`, `mini`, `nano`, `preview`);
  - keep canonical acronyms (`GPT`, `GLM`, `MiMo`, `MiniMax`, `DeepSeek`, `Kimi`, `Gemini`);
  - fall back to the raw id when the result is empty.
- Expected examples: `claude-sonnet-4-5` → `Sonnet 4.5`; `claude-opus-4-1` → `Opus 4.1`;
  `gpt-5-mini` → `GPT-5 Mini`; `gemini-2.5-flash` → `Gemini 2.5 Flash`;
  `deepseek-v4.1-flash` → `DeepSeek V4.1 Flash`; `anthropic/claude-sonnet-4.5` → `Sonnet 4.5`.
- Unit tests cover every static catalog id plus live-style ids with dates.

### Combo extensions

Extend `shell/Combo.tsx` rather than duplicating the listbox:

- `ComboOption` gains `disabled?: boolean` and `icon?: ReactNode`.
- New props: `triggerIcon?: ReactNode`, `renderTriggerValue?: (value: string) => ReactNode`,
  `hideChevron?: boolean`, `footer?: ReactNode`, `triggerClassName?: string`.
- Disabled options are skipped by keyboard navigation and render dimmed with their hint.
- The popup keeps search, groups, active-row scrolling, and outside close.

### Composer pair

Replace the two `Combo`s in `SessionPane.tsx` with:

```tsx
<div className="composer-pickers">
  <Combo className="inline-picker thinking-picker" triggerIcon={<Icon name="brain" />} ... />
  <Combo className="inline-picker model-picker"
         triggerIcon={<ProviderGlyph provider={currentProvider} />}
         renderTriggerValue={(value) => modelLabel(value, currentProvider)} ... />
</div>
```

- Add a `brain` glyph to `workspace/Icon.tsx`.
- CSS:
  - `.composer-pickers { display: inline-flex; align-items: center; gap: 4px; }`
  - `.inline-picker .combo-trigger { background: transparent; border: 0; padding: 4px 6px;
    font-size: 12px; color: var(--fg-3); }`
  - `.inline-picker .combo-trigger:hover { background: var(--hover); color: var(--fg); }`
  - `.inline-picker + .inline-picker::before { content: "·"; color: var(--fg-3); margin: 0 2px; }`
  - Remove the old `.combo-model`/`.combo-thinking` composer rules and the extra font-size override
    in the `.session` block.
- Both triggers keep their accessible names ("Thinking level for this session", "Model for this
  session"). `aria-haspopup="listbox"` stays from `Combo`.

### Model menu content

- Build groups: Recent (last 5 used, filtered to configured models), then provider groups.
- Option label = `modelLabel`; hint = exact id.
- Recent ids persist in `localStorage["grove.recentModels"]`; prune ids no longer present.
- Unconfigured providers render dimmed with hint "Add a key in Settings"; the popup footer has a
  "Providers" button that opens Settings → Providers.
- Picking a model:
  - same provider → `setModel` (hot);
  - different provider → `switchModel` (lossless switch, queued if busy);
  - push the id to recents; toast `Session now runs on <Provider> · <Model Label>`.

## E2. Same-provider hot swap

### Trait

Add to `AgentSession` in `providers/mod.rs`:

```rust
async fn set_model(&mut self, model: String) -> Result<(), ProviderError>;
```

- Default implementation returns `ProviderError::Unsupported` so stubs do not break.
- `AgentSessionImpl` implements it by assigning `self.model` (the field at `agent.rs:180`). `send`
  already clones the model into the spawned turn (`agent.rs:846`), so the change affects only the
  next turn.
- `OfflineSession` accepts it and updates its model for tests; `StubSession` records it.

### Manager and store

- `Store::set_session_model(id, model)` updates `model` and `updated_at`.
- `SessionManager::set_model(id, model, thinking: Option<ThinkingLevel>)`:
  - load the row; reject empty;
  - if live, lock the command mutex and call `handle.set_model`;
  - persist model and, when provided, thinking in one update;
  - return `SessionDetail`.
- The UI computes the nearest allowed thinking level for the new model from `modelThinking` and
  passes it, so the model/thinking pair changes atomically. If capability data is missing, leave the
  level untouched.
- New command `session_set_model`; add to the command map, fake bridge, and contract types if the
  request shape is new.
- The UI toast is the only transcript feedback; no `ModelChanged` event is needed for a same-provider
  swap.

## E3. Cross-provider lossless switch

### History handoff

- Add `async fn history(&self) -> Vec<wire::Message>` to `AgentSession` (or `take_history` if the
  agent is stopped).
- `AgentSessionImpl` clones `self.history.lock()`.
- Add `history: Vec<wire::Message>` to `StartContext` (default empty).
- `WireProvider::start`: when `ctx.history` is non-empty, seed `history` from it instead of building
  from `ctx.prompt`; otherwise keep the current prompt replay.
- Update every `StartContext` literal (`manager.rs`, `session_context.rs` tests, provider tests).

### Manager switch

In `switch_provider`:

- If the session is live and idle, capture `history` before `stop`.
- Stop as today, persist provider/model, keep the context note for a real provider change.
- Launch with `history: captured`, `prompt: None`; fall back to today's `render_tail` path when the
  session is stopped or history is empty.
- If the session is busy or awaiting, return a typed `conflict` error; the UI queues and retries at
  idle. Do not stop a busy session just to switch.
- Trim the history to the last complete assistant message before handoff, so a mid-tool-call tail is
  never sent to another provider.

### UI queue

In `sessionStore`:

- `pendingModel: { provider, model } | null`.
- `switchModel(bridge, provider, model)`:
  - same provider → `setModel`;
  - different provider and `detail.status` is `busy`/`starting`/`awaiting` → set `pendingModel` and
    show the chosen label with a small "waiting for this turn" tooltip/dot;
  - otherwise call `switchProvider` immediately.
- On a `status` event of `idle` or `ready`, if `pendingModel` exists, apply it.
- On `exited`/`error`, clear the pending switch (no history to hand off).
- On archive/delete, clear it.
- The existing toast tells the user when the switch lands.

### Regression note

The same stop-path fix from C5 is required here: never abort a turn while a permission or question
is pending; the queue waits for idle, and every stop path clears `awaiting` via a published `Exited`.

## Tests

- `modelLabel` table tests for every catalog id.
- Combo: disabled options skipped, search, recent group, hint rendering.
- Same-provider: `session_set_model` updates the live model, does not call `start`, keeps one
  `StartContext`, persists the model, and the next wire request carries the new model while the
  previous history is intact.
- Thinking clamp: switching to a model with `max` only clamps and persists atomically.
- Cross-provider: history is exported, the new `StartContext` carries it and `prompt: None`, the
  transcript is not replayed, and the provider note is emitted.
- Queue: selecting a cross-provider model during a busy turn stores a pending switch; it applies on
  `idle`; cancelling or archiving clears it.
- Stopped session: cross-provider falls back to the existing transcript replay.
- Fake bridge handlers for `session_set_model` and the pending path.

## Risks / ADR

- No token accounting: switching a long conversation to a smaller-context model can fail on the next
  request. This plan does not add context-window checks; models.dev exposes limits and a follow-up
  should warn/refuse when the transcript exceeds the target window. Output ceilings are also wire
  constants (8,192 for Anthropic/Google).
- The wire clients are stateless per turn, so history handoff is sound, but Anthropic thinking blocks
  are already dropped by the decoder; do not claim reasoning signatures survive a switch.
- Write **ADR 0005 — lossless cross-provider model handoff**. It is hard to reverse (trait +
  `StartContext` contract), surprising (a provider switch that is not a fresh conversation), and a
  real trade-off against keeping the provider modules fully isolated.

---

# Cross-cutting work

## Contract and fixtures

Every DTO change needs a zod schema, a Rust serde struct, a fixture, and the fake bridge:

- `Session.archivedAt`, `Session.titleSource`.
- `FsRoots`, `FsRootKind`, `fs_create_dir` request/result.
- `settings.titleModel`.
- `TitleChanged` in the session event union.
- New command types: `session_rename`, `session_archive`, `session_restore`, `session_delete`,
  `session_set_model`, `session_generate_title`, `fs_list_roots`, `fs_create_dir`,
  `title_model_suggest`, `app_reveal_path`, `project_remove`.

## Shared-file seams

Keep each ticket's edits to shared files minimal and in the established order:

- `commands/mod.rs` — one `generate_handler!` line per command.
- `ipc/commands/index.ts` — one map entry per command.
- `packages/contract/src/index.ts` — one export per new module/type.
- `sessionStore.ts` — one action per feature; do not refactor unrelated actions.
- `design.css` — append per-surface blocks; do not reorder existing selectors.

## Docs

- Update `architecture.md` §IPC/commands and the settings table for the new commands and keys.
- Update `GLOSSARY.md` with the five terms above.
- Write ADR 0004 (auto titles), ADR 0005 (lossless handoff), and optionally ADR 0006 (archive is
  soft state; permanent delete only from Archive).
- No new npm dependency, no new Tauri plugin, no new network call at boot.

## Verification

Run from the repo root:

```
bun run typecheck
bun run test
bun run build
cargo nextest run -p grove-core
cargo test -p grove-core --doc
cargo check -p grove-desktop
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Manual matrix:

- 1920/1440/1280/900/640 widths for the chat column.
- Add a project from `D:` while Grove runs from `C:`; paste a path; create a folder; select it.
- Right-click every surface, including inside the add-project and settings dialogs; keyboard-only
  navigation.
- Rename inline, archive a running and a stopped session, restore, delete.
- Auto-name a new session; rename it; regenerate with AI; force a failure and check the toast.
- Switch models within a provider mid-turn and across providers mid-turn; verify the queue and the
  transcript continuity.
- Restart the app and check archived sessions, titles, and the last selected session.

## Out of scope

- Token/context-window accounting for model swaps.
- Automatic archive retention or cleanup.
- Hidden-file toggles in the folder picker.
- `file://` URL normalization.
- Session tabs or drag-and-drop reordering.
- ClickUp ticket creation — the user will take this plan to the spec manually.
