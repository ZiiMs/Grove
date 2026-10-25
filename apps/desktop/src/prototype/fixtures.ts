/**
 * PROTOTYPE — throwaway. One fixture set per panel, shared by that panel's three variants, so a
 * variant can only ever differ from its siblings by structure, never by content.
 */

export interface Turn {
  id: string;
  role: "user" | "agent";
  harness: string;
  glyph: string;
  time: string;
  text: string;
  streaming?: boolean;
  thinking?: string;
  tool?: { name: string; arg: string; result: string; state: "run" | "ok" | "err" };
  approval?: boolean;
  error?: { title: string; detail: string };
}

export const thread: Turn[] = [
  {
    id: "t1",
    role: "user",
    harness: "you",
    glyph: "ZY",
    time: "14:02",
    text: "Add the ACP adapter behind HarnessAdapter and keep the other four untouched.",
  },
  {
    id: "t2",
    role: "agent",
    harness: "Claude Code",
    glyph: "CC",
    time: "14:02",
    text: "The seam already fits — ACP needs supports_resume and a stdio transport. Writing the adapter now.",
    streaming: true,
    thinking: "Spec 03 pins the adapter seam: one trait, no cross-adapter imports. Checking the existing five…",
    tool: {
      name: "read",
      arg: "crates/grove-core/src/adapters/mod.rs",
      result: "pub trait HarnessAdapter: Send + Sync {\n    fn id(&self) -> &'static str;\n    fn spawn(&self, cfg: SessionConfig) -> Result<SessionHandle>;\n}",
      state: "ok",
    },
  },
  {
    id: "t3",
    role: "agent",
    harness: "Claude Code",
    glyph: "CC",
    time: "14:03",
    text: "Adapter drafted. Running the conformance suite before I touch the registry.",
    approval: true,
  },
  {
    id: "t4",
    role: "agent",
    harness: "Codex",
    glyph: "CX",
    time: "14:05",
    text: "",
    error: {
      title: "Session exited with code 2",
      detail: "app-server handshake timed out after 15s — the harness may not be installed.",
    },
  },
];

export interface DiffFile {
  path: string;
  added: number;
  removed: number;
  rows: { kind: "ctx" | "add" | "del" | "hunk"; ln: number; text: string }[];
}

export const diff: DiffFile = {
  path: "crates/grove-core/src/adapters/acp.rs",
  added: 7,
  removed: 1,
  rows: [
    { kind: "hunk", ln: 0, text: "@@ -12,7 +12,9 @@ impl HarnessAdapter for AcpAdapter" },
    { kind: "ctx", ln: 12, text: "    fn id(&self) -> &'static str {" },
    { kind: "del", ln: 13, text: '        "acp"' },
    { kind: "add", ln: 13, text: '        "acp"' },
    { kind: "add", ln: 14, text: "    }" },
    { kind: "add", ln: 15, text: "" },
    { kind: "add", ln: 16, text: "    fn supports_resume(&self) -> bool {" },
    { kind: "add", ln: 17, text: "        true" },
    { kind: "ctx", ln: 18, text: "    }" },
  ],
};

export const changedFiles = [
  { path: "adapters/acp.rs", added: 41, removed: 0, state: "new" },
  { path: "adapters/mod.rs", added: 3, removed: 1, state: "mod" },
  { path: "contract/fixtures/session.json", added: 12, removed: 0, state: "new" },
  { path: "apps/desktop/src/panels/sessions/index.tsx", added: 6, removed: 2, state: "mod" },
];

export const sessionList = [
  { id: "s1", title: "ACP adapter behind HarnessAdapter", harness: "Claude Code", glyph: "CC", state: "live", elapsed: "4m 12s", tokens: "18.4k" },
  { id: "s2", title: "Contract fixture for session DTO", harness: "Codex", glyph: "CX", state: "warn", elapsed: "1m 03s", tokens: "6.1k" },
  { id: "s3", title: "Spec graph tree — edge labels", harness: "OpenCode", glyph: "OC", state: "idle", elapsed: "24m", tokens: "31.2k" },
  { id: "s4", title: "Skill catalog pagination", harness: "pi", glyph: "PI", state: "ok", elapsed: "2h", tokens: "12.7k" },
];

export const specs = [
  { id: "goal", depth: 0, name: "goal-and-requirements.md", type: "goal", active: false, edge: "" },
  { id: "arch", depth: 0, name: "architecture.md", type: "arch", active: true, edge: "" },
  { id: "a03", depth: 1, name: "03-adapter-interface", type: "module", active: false, edge: "depends-on" },
  { id: "a04", depth: 1, name: "04-ipc-contract", type: "module", active: false, edge: "implements" },
  { id: "a04b", depth: 2, name: "contract-fixtures", type: "sub", active: false, edge: "references" },
  { id: "a05", depth: 1, name: "05-session-persistence", type: "module", active: false, edge: "depends-on" },
  { id: "a06", depth: 1, name: "06-panel-seam", type: "module", active: false, edge: "references" },
];

export const specDoc = {
  id: "05-session-persistence",
  title: "Session persistence",
  status: "active",
  links: ["depends-on: 03-adapter-interface", "references: 04-ipc-contract", "covers: sessions panel"],
  body: [
    "Sessions are rows in a local SQLite database, not files. The event log is append-only and the panel replays it, so a restart shows the same transcript without asking the harness to resume.",
    "Persistence owns the shape of a turn: role, harness, tool calls, and the diff a turn produced. It never owns the harness's own history — resume is delegated to the adapter.",
    "Two invariants matter. A turn is written once and never edited. A resumed session must not duplicate turns on replay, which is what the fixture in 04-ipc-contract pins.",
  ],
};

export const skills = [
  { name: "code-review", source: "mattpocock/skills", tag: "review", installed: ["claude"], blurb: "Two-axis review: standards and spec conformance, in parallel sub-agents." },
  { name: "brainstorming", source: "anthropics/skills", tag: "plan", installed: ["claude", "codex"], blurb: "Interview before implementing; settles the design tree in rounds." },
  { name: "tdd", source: "mattpocock/skills", tag: "build", installed: [], blurb: "Red-green-refactor with integration tests as the primary loop." },
  { name: "diagnosing-bugs", source: "anthropics/skills", tag: "debug", installed: ["pi"], blurb: "A diagnosis loop for hard bugs and performance regressions." },
  { name: "spec-graph", source: "grove/skills", tag: "specs", installed: ["claude", "pi", "opencode"], blurb: "Read, link and validate the project's spec graph from a session." },
  { name: "shipping-a-pr", source: "mattpocock/skills", tag: "ship", installed: [], blurb: "Create, sync, monitor and repair a pull request." },
];

export const installTargets = ["claude", "codex", "pi", "opencode", "acp"];

export const plan = [
  { id: "p1", text: "Pin spec 03 to the ACP session", state: "done" },
  { id: "p2", text: "Add the adapter behind HarnessAdapter", state: "done" },
  { id: "p3", text: "Contract fixture for the session DTO", state: "now" },
  { id: "p4", text: "Wire resume through the sidecar", state: "todo" },
  { id: "p5", text: "Diff card + approval flow", state: "todo" },
  { id: "p6", text: "Replay test: restart keeps the transcript", state: "blocked" },
];

export const mcpServers = [
  { name: "filesystem", transport: "stdio", state: "ok", tools: ["read_file", "write_file", "list_dir", "move_file", "search"] },
  { name: "github", transport: "http", state: "ok", tools: ["create_pr", "review_pr", "list_issues", "comment", "checks"] },
  { name: "linear", transport: "http", state: "warn", tools: ["list_issues", "update_issue", "attach"] },
  { name: "sqlite", transport: "stdio", state: "idle", tools: ["query", "schema", "insert"] },
  { name: "sentry", transport: "sse", state: "err", tools: [] },
];

export const gitRows = [
  { branch: "grove-project-setup", worktree: "workspace-3", sync: "ahead 3", state: "ok", pr: "#12 draft", checks: "1/4" },
  { branch: "feat/acp-adapter", worktree: "acp-root", sync: "ahead 1", state: "live", pr: "#14 open", checks: "3/4" },
  { branch: "fix/contract-zod", worktree: "zod-fix", sync: "behind 2", state: "warn", pr: "#11 review", checks: "4/4" },
  { branch: "main", worktree: "main", sync: "clean", state: "ok", pr: "—", checks: "—" },
];

export const harnesses = [
  { id: "claude", name: "Claude Code", adapter: "stream-json", state: "ok", detail: "claude 0.9.2 found on PATH" },
  { id: "codex", name: "Codex", adapter: "app-server", state: "warn", detail: "installed, app-server handshake slow (4.2s)" },
  { id: "opencode", name: "OpenCode", adapter: "http+sse", state: "ok", detail: "server on 127.0.0.1:4123" },
  { id: "pi", name: "pi", adapter: "node sdk", state: "ok", detail: "sidecar ready, shared ~/.pi" },
  { id: "acp", name: "ACP (generic)", adapter: "json-rpc", state: "err", detail: "no agent configured" },
];

export const settingsGroups = [
  {
    id: "general",
    title: "General",
    fields: [
      { label: "Default harness", kind: "select", value: "Claude Code" },
      { label: "Default install location", kind: "select", value: ".claude/skills — project" },
      { label: "Reopen last session on launch", kind: "switch", value: "on" },
    ],
  },
  {
    id: "secrets",
    title: "Secrets",
    fields: [
      { label: "GitHub token", kind: "secret", value: "ghp_••••••••••4f2a" },
      { label: "Anthropic key", kind: "secret", value: "sk-ant-••••••7c1" },
      { label: "Linear key", kind: "empty", value: "not set" },
    ],
  },
  {
    id: "appearance",
    title: "Appearance",
    fields: [
      { label: "Theme", kind: "select", value: "Ember" },
      { label: "Radius", kind: "select", value: "0px" },
      { label: "Density", kind: "select", value: "Cozy" },
    ],
  },
  {
    id: "catalog",
    title: "Skill catalog",
    fields: [
      { label: "Catalog sources", kind: "text", value: "anthropics/skills, mattpocock/skills" },
      { label: "Cache when offline", kind: "switch", value: "on" },
    ],
  },
];

export function stateDot(state: string) {
  return state === "live" ? "is-live" : `is-${state}`;
}

/* ---------- pass 3: project tree + skill catalogs ---------- */

export interface NavSession {
  id: string;
  title: string;
  harness: string;
  glyph: string;
  state: string;
  elapsed: string;
}

export interface NavWorktree {
  id: string;
  name: string;
  branch: string;
  state: string;
  sessions: NavSession[];
}

export interface NavProject {
  id: string;
  name: string;
  path: string;
  state: string;
  worktrees: NavWorktree[];
}

export const looseChats: NavSession[] = [
  { id: "c1", title: "How do ACP and stream-json differ?", harness: "Claude Code", glyph: "CC", state: "ok", elapsed: "3m" },
  { id: "c2", title: "Draft a skill for spec review", harness: "pi", glyph: "PI", state: "idle", elapsed: "1h" },
];

export const projects: NavProject[] = [
  {
    id: "grove",
    name: "grove",
    path: "D:/Dev/grove",
    state: "ok",
    worktrees: [
      {
        id: "workspace-3",
        name: "workspace-3",
        branch: "grove-project-setup",
        state: "live",
        sessions: [
          { id: "s1", title: "ACP adapter behind HarnessAdapter", harness: "Claude Code", glyph: "CC", state: "live", elapsed: "4m" },
          { id: "s2", title: "Contract fixture for session DTO", harness: "Codex", glyph: "CX", state: "warn", elapsed: "1m" },
        ],
      },
      {
        id: "acp-root",
        name: "acp-root",
        branch: "feat/acp-adapter",
        state: "ok",
        sessions: [
          { id: "s3", title: "Wire resume through the sidecar", harness: "pi", glyph: "PI", state: "ok", elapsed: "22m" },
        ],
      },
      {
        id: "zod-fix",
        name: "zod-fix",
        branch: "fix/contract-zod",
        state: "warn",
        sessions: [],
      },
    ],
  },
  {
    id: "thinkrail",
    name: "thinkrail",
    path: "D:/Dev/thinkrail",
    state: "idle",
    worktrees: [
      {
        id: "tr-main",
        name: "main",
        branch: "main",
        state: "ok",
        sessions: [
          { id: "s4", title: "Spec graph viewer polish", harness: "Claude Code", glyph: "CC", state: "idle", elapsed: "2h" },
        ],
      },
    ],
  },
];

export interface Catalog {
  id: string;
  name: string;
  owner: string;
  count: number;
  installed: number;
  blurb: string;
  updated: string;
}

export const catalogs: Catalog[] = [
  { id: "anthropics", name: "skills", owner: "anthropics", count: 34, installed: 2, blurb: "First-party set — brainstorming, tdd, diagnosing-bugs, research.", updated: "3d ago" },
  { id: "mattpocock", name: "skills", owner: "mattpocock", count: 21, installed: 1, blurb: "Review, shipping-a-pr, scaffold-exercises and the rest of the set.", updated: "yesterday" },
  { id: "claude-plugins", name: "plugins", owner: "anthropics", count: 88, installed: 0, blurb: "Bundled Claude Code plugins that expose skills.", updated: "1w ago" },
  { id: "codex-skills", name: "codex-skills", owner: "openai", count: 12, installed: 1, blurb: "Skills written for Codex workflows.", updated: "5d ago" },
  { id: "grove", name: "skills", owner: "grove", count: 6, installed: 3, blurb: "Skills that read this project's own spec graph.", updated: "today" },
  { id: "awesome", name: "awesome-agent-skills", owner: "community", count: 240, installed: 0, blurb: "Aggregated index — unvetted, sorted by stars.", updated: "2d ago" },
];

export interface CatalogSkill {
  catalog: string;
  name: string;
  blurb: string;
  installed: string[];
  updated: string;
}

export const catalogSkills: CatalogSkill[] = [
  { catalog: "anthropics", name: "brainstorming", blurb: "Interview before implementing; settle the design tree in rounds.", installed: ["claude", "codex"], updated: "3d ago" },
  { catalog: "anthropics", name: "tdd", blurb: "Red-green-refactor with integration tests as the primary loop.", installed: [], updated: "3d ago" },
  { catalog: "anthropics", name: "diagnosing-bugs", blurb: "A diagnosis loop for hard bugs and performance regressions.", installed: ["pi"], updated: "1w ago" },
  { catalog: "anthropics", name: "research", blurb: "Primary-source research captured as a file in the repo.", installed: [], updated: "2w ago" },
  { catalog: "mattpocock", name: "code-review", blurb: "Two-axis review: standards and spec conformance, in parallel.", installed: ["claude"], updated: "yesterday" },
  { catalog: "mattpocock", name: "shipping-a-pr", blurb: "Create, sync, monitor and repair a pull request.", installed: [], updated: "yesterday" },
  { catalog: "codex-skills", name: "codex-plan-mode", blurb: "Plan-mode discipline for long Codex sessions.", installed: ["codex"], updated: "5d ago" },
  { catalog: "grove", name: "spec-graph", blurb: "Read, link and validate the project's spec graph from a session.", installed: ["claude", "pi", "opencode"], updated: "today" },
  { catalog: "grove", name: "promote-comments", blurb: "Move rationale out of code comments into the owning spec.", installed: [], updated: "today" },
];
