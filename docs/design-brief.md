# Grove — UI design brief & prompt

> **Superseded (second build).** The design in `design-reference/` is now the production UI: the
> project tree, always-chat centre, Git · Plan · Spec context column and the settings sheet described
> in `docs/adr/0002-workspace-projects-and-the-redesigned-shell.md`. This file is kept as the brief the
> first build was designed against; its "current state" section describes the retired prototype.

Two things in one file: a **condensed prompt** (paste this into a design chat), and the **full brief**
underneath (send this to a designer, or attach it). Everything below describes the interface that
exists today in the prototype under `apps/desktop/src/prototype/` — treat it as the current state, not
as the ceiling.

---

## 1. Condensed prompt (paste this)

> You are designing the UI for **Grove**, an open-source desktop app (Tauri 2 + React) that runs AI
> coding agents from several harnesses — Claude Code, Codex, OpenCode, `pi`, and any ACP agent — in one
> window, and keeps the project's skills, specs, plan and git state in view while the agent works.
>
> The design language is **Ember**: warm near-black surfaces, amber accent, **0px corner radius
> everywhere**, cozy density, 13px base type. Full token list below — please use it verbatim.
>
> The shell is three columns: **232px left project tree · centre chat · 320px right context panel**,
> with a 26px status bar. The centre is always the chat; there is no breadcrumb bar and no panel tab
> strip. Two surfaces are not in the columns at all: the **skill catalog** and the **MCP inspector**,
> which live as sections of a near-fullscreen **Settings modal**.
>
> Please redesign this shell and the surfaces listed in §5 against the tokens and rules in §3–§6,
> keeping the interaction contract in §6 intact. Deliver frames at 1280×800 and 1440×900 plus the
> states in §7. Where something is marked **settled**, keep it; where it is marked **open**, push back
> with your own alternative.

---

## 2. What Grove is (product context)

- **Users**: developers who already run two or more agent CLIs, and who lose project context every time
  a session restarts or they switch harness.
- **The product's reason to exist**: the agent harnesses each keep their own history, their own skills
  folder and their own idea of project context. Grove is the shared layer: one window for many
  harnesses, plus a skill catalog, a spec graph, a TODO plan, git/PR work and MCP tooling.
- **Intent of record**: `goal-and-requirements.md` at the repo root. Read it before making structural
  suggestions — especially its non-goals.
- **Neighbours, for register rather than copying**: OpenChamber (Tauri, git hosts, PR stacks),
  ThinkRail (git-worktree workspaces, spec graph), Harnss / Conduit / Hydra / desktop-cc-gui / Codemux
  (multi-agent desktop apps, mostly terminal-wrapped agents).
- **Platform**: Windows first, macOS and Linux to follow. Local-first, no account, no cloud sync.

## 3. Design language — Ember (settled tokens)

Dark scheme, warm neutrals rather than blue-grey. Use these exact values; they are already implemented
as CSS custom properties.

| Token | Value | Use |
|---|---|---|
| `--bg` | `#12100d` | app background |
| `--bg-sunken` | `#0b0a08` | inputs, code, canvas, wells |
| `--bg-raised` | `#1a1713` | nav column, toolbars, card headers |
| `--bg-overlay` | `#211d18` | menus, toasts, popovers |
| `--surface` | `#17140f` | cards, panels, list surfaces |
| `--surface-hover` | `#1f1b16` | row and control hover |
| `--border` | `#2c2620` | default hairlines |
| `--border-strong` | `#3f372d` | inputs, dividers that need to read |
| `--ink` | `#f1eae0` | primary text |
| `--ink-muted` | `#ab9f8d` | secondary text |
| `--ink-faint` | `#746a5c` | metadata, placeholders, disabled |
| `--accent` | `#f0a04b` | amber — selection, primary action, live state |
| `--accent-ink` | `#191007` | text on accent fills |
| `--danger` | `#e2725b` | errors, destructive, deletions |
| `--success` | `#9bbf6b` | added lines, passing checks, connected |
| `--warning` | `#f2c14e` | "needs you", degraded, approvals |
| `--info` | `#7fb8c9` | hunk headers, neutral notices |
| `--shadow` | `0 12px 32px rgb(0 0 0 / 50%)` | overlays, modal, drawers |

**Accent alternatives** (a user-facing setting; the UI must survive any of them): mint `#6ee7a8`,
amber `#f0a04b` (default), cyan `#4cc9f0`, violet `#b28dff`, rose `#fb7185`, lime `#a3e635`.

**Light counterpart** — designed, not inverted; direction only, refine it: warm paper `#f7f4ee`
background, `#fffdf9` raised, `#e0d8cb` borders, `#24201a` ink, `#4c5fd5` indigo accent.

### Shape and density (settled decisions)

- **Corner radius is 0px on everything.** Buttons, cards, inputs, modal, avatars, chips, badges,
  switches, progress bars — square. This is a deliberate decision, not an oversight. Do not round
  corners "just slightly"; if you think it reads too hard, tell us why instead of softening it.
- **Density: cozy.** Row height 32px, padding 10/14px, gap 8px, base type 13px. Compact (26px rows,
  12.5px) and comfy (40px, 14px) are also shipping settings — designs must hold at all three.
- Spacing scale: 4 / 8 / 12 / 16 / 24 / 32px.
- Type scale: display 22 · title 15/620 · body 13 · small 11.5 · caps 9.5 (uppercase, 0.1em tracking).
- UI stack: `"Segoe UI Variable Text", "Segoe UI", system-ui`. Mono: `"Cascadia Code", "JetBrains Mono"`.

## 4. Shell layout (settled structure)

```
┌──────────── 232 ────────────┬────────────── 1fr ──────────────┬──── 320 ────┐
│ ◆ Grove        ＋ ⌯ │ ✦ ⬡ │ slim chat header (40px)           │ Plan Specs  │
│ ─────────────────────────── │  avatar · title · harness chips  │ Changes Git │
│ ▸ Chats            2        │  · approval badge      · toggle  │ ─────────── │
│   chat row                  ├──────────────────────────────────┤ pinned block│
│ ▸ Projects         2        │                                  │ (what needs │
│   ▸ grove                 15px│        transcript              │  you first) │
│     ▸ workspace-3 (branch)   │        tool cards, diffs,        │ rows        │
│       session row  (live)    │        approvals, thinking       │             │
│     ▸ acp-root               │                                  │             │
│       session row            ├──────────────────────────────────┤             │
│   ▸ thinkrail                │ composer: attach · skill · spec  │             │
│ ─────────────────────────── │           model pick · send      │             │
│ ⚙ (settings, icon only)     ├──────────────────────────────────┴─────────────┤
│                             │ status bar 26px: theme · branch · tokens · ▥    │
└─────────────────────────────┴──────────────────────────────────────────────────┘
```

- **Left nav = project tree only.** Header carries four global actions: add project, new chat (a session
  with no project directory), skill catalog, MCP. Settings is a lone icon in the footer.
- **Tree**: `Chats` group (directory-less sessions) above `Projects`; each project expands to its
  **worktrees**, each worktree to its **sessions**. One 15px indent step per level with drawn
  connectors (vertical run, elbow into the row). Project rows read heaviest, worktree rows carry the
  branch in mono, session rows carry a state dot and elapsed time.
- **Centre = chat, always.** No breadcrumb bar. No panel tab strip. The slim header is the only chrome
  above the transcript.
- **Right panel = context, not navigation**: four tabs (Plan · Specs · Changes · Git) with live counts,
  collapsible to a 40px strip that keeps its glyphs and counts.
- **Modal** = near-fullscreen (`min(1120px, 92vw) × min(840px, 90vh)`), scrim + blur, left section nav,
  Esc or ✕ to close.

## 5. Surfaces to design

1. **Nav tree** — chats/projects/worktrees/sessions, selected and hover states, collapsed/expanded,
   long-name truncation, empty project, worktree with no sessions, 40+ projects scrolling, an agent
   that needs attention.
2. **Chat** — slim header; transcript with user turns, agent turns (streaming), thinking blocks,
   tool-call cards (with params and result), diff cards (line numbers, add/remove), approval cards,
   failed-session error; composer with attachments, skill picker, spec picker, model pick, send;
   queued message; long transcript.
3. **Right panel** — four tabs with counts, pinned "what needs you" block, item rows, the 40px collapsed
   strip, empty state per tab.
4. **Settings modal** — six sections: General, Secrets, Appearance, **Skill catalog**, **MCP**,
   Harnesses (doctor). The catalog is a catalog list + skill table with per-harness install state. MCP
   is servers + tools with per-tool toggles and a failed-connection state.
5. **Status bar** — branch, harness, tokens, sync state, panel toggle.
6. **States off the happy path** — empty (no projects, no chats, no specs, empty catalog), loading
   skeletons, offline catalog with cache notice, MCP connection failure, harness not installed, diff
   conflict, approval denied, session exited mid-turn.

## 6. Interaction contract (settled — keep it)

- The centre is chat; opening the skill catalog, MCP or settings **opens the modal at that section** and
  leaves the chat mounted underneath.
- Selecting a project / worktree / session in the tree changes what the centre shows; it never opens a
  new window or a different app surface.
- The right panel is per-session context. Its tabs never navigate away from the chat.
- Collapsing the right panel must not move the chat's own header or the composer.
- Esc closes the modal; clicking the scrim closes it; the close control is always in the modal header.
- Destructive actions (deny, discard, remove server, detach skill) are never the accent-primary button
  in a group of three or more.
- Nothing in the product suggests an account, a cloud, or model billing — Grove is local-first and
  bring-your-own-keys.

## 7. What we want back

- Frames at **1280×800** and **1440×900**, plus the intended behaviour at **1024px** width (narrow
  desktop): what collapses first, what truncates, what hides.
- For each surface: default, hover, focus-visible, selected/active, disabled, loading, empty and error.
- The **modal at both catalog and MCP sections**, since those are the densest screens.
- A **token sheet** if you change any value, so the CSS variables can be updated in one place.
- One page of rationale: what you changed, what you deliberately kept, and where you think we are wrong.
- Interactive or component-level deliverables beat static comps; a clickable prototype of the shell
  plus the modal is worth more than ten polished screens.

## 8. Settled vs open

**Settled — keep:** 0px radius · Ember palette and token names · three-column shell with a 232px nav and
320px panel · chat-always-centre · no breadcrumb, no panel tab strip · Skills/MCP/Settings inside one
modal · tree nesting project → worktree → session · cozy density as default · amber as default accent.

**Open — redesign freely:** typography personality (we are on a system stack; a shipped face is on the
table) · iconography (currently mixed unicode glyphs — a real icon set would help) · motion and
transition language · empty-state and error-state language · the compact item design inside the right
panel · catalog table density and the install affordance · the composer's control layout · how the
approval card escalates without shouting.

**Not in scope:** mobile, onboarding flow, a full editor, model/provider billing, theming marketplace.

## 9. Run the current state

```sh
npm ci
npm run dev        # then open http://localhost:1420/?prototype=1
```

Prototype routes: `?section=` one of `theme, shell, sidepanel, controls, chat, data, sessions, plan,
specs, changes, git, skills, mcp, settings`; `?variant=A|B|C` for the layouts that still have
alternatives. This surface is throwaway — do not treat its code or CSS class names as the production
implementation, and ignore the floating pill at the bottom (it is our variant switcher, not product UI).
