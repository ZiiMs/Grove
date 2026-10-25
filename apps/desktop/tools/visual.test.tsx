import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { readdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import type {
  DoctorReport,
  GitBranch,
  GitWorktree,
  HarnessEvent,
  HarnessInfo,
  McpDiscovery,
  Plan,
  Project,
  SessionDetail,
  SkillCatalog,
  SpecGraph,
} from "@grove/contract";
import App from "../src/App";
import { BridgeProvider } from "../src/ipc/context";
import { createFakeBridge, fakeSession, fakeSummary } from "../src/test-utils/fakeBridge";
import { useSessionStore } from "../src/state/sessionStore";
import { useWorkspaceStore } from "../src/state/workspaceStore";
import { useContextStore } from "../src/state/contextStore";
import { useUiStore } from "../src/state/uiStore";

const project: Project = {
  id: "project-1",
  name: "grove",
  path: "C:/code/grove",
  remote: "acme/grove",
  defaultBranch: "feat/harness-multiplexer",
  isGit: true,
  createdAt: "2025-01-01T00:00:00.000Z",
  updatedAt: "2025-01-01T00:00:00.000Z",
};

const worktree: GitWorktree = {
  path: "C:/code/grove",
  head: "1111111111111111111111111111111111111111",
  branch: "feat/harness-multiplexer",
  bare: false,
  detached: false,
  current: true,
};

const branch: GitBranch = {
  name: "feat/harness-multiplexer",
  remote: null,
  current: true,
  upstream: null,
  ahead: 3,
  behind: 0,
};

const session = fakeSession({
  id: "session-1",
  projectId: project.id,
  harness: "claude",
  model: "claude-sonnet-4-5",
  title: "OAuth device flow",
  status: "idle",
  awaiting: true,
  cwd: worktree.path,
});

const events: HarnessEvent[] = [
  {
    at: "2025-01-01T09:40:58.000Z",
    kind: "message",
    role: "user",
    blocks: [
      {
        type: "text",
        text: "Wire up the GitHub OAuth device flow for sign-in. Reuse the existing keychain helper — no new dependencies.",
      },
    ],
  },
  {
    at: "2025-01-01T09:41:00.000Z",
    kind: "message",
    role: "assistant",
    blocks: [
      {
        type: "text",
        text: "Reading the session module and the keychain helper first. The device flow needs a poll loop with a slow_down backoff, so I want to see how session state is already threaded before adding a second async loop.",
      },
    ],
  },
  {
    at: "2025-01-01T09:41:07.000Z",
    kind: "tool_call",
    call_id: "read-1",
    name: "Read",
    title: "session.rs",
  },
  {
    at: "2025-01-01T09:41:07.200Z",
    kind: "tool_result",
    call_id: "read-1",
    status: "ok",
    summary: "/// Spawn the underlying harness process and start the transport loop.",
  },
  {
    at: "2025-01-01T09:41:09.000Z",
    kind: "tool_call",
    call_id: "grep-1",
    name: "Grep",
    title: "keychain_store",
  },
  {
    at: "2025-01-01T09:41:09.300Z",
    kind: "tool_result",
    call_id: "grep-1",
    status: "ok",
    summary: "3 matches\ncrates/auth/src/lib.rs:41",
  },
  {
    at: "2025-01-01T09:41:22.000Z",
    kind: "message",
    role: "assistant",
    blocks: [
      {
        type: "text",
        text: "The keychain helper already returns a typed Secret, so the device flow can reuse it untouched. I added oauth/device.rs with the poll loop and wired a start_device_flow command. Now I want to run the crate tests before touching the command layer.",
      },
    ],
  },
  {
    at: "2025-01-01T09:41:23.000Z",
    kind: "tool_call",
    call_id: "write-1",
    name: "Write",
    title: "oauth/device.rs",
  },
  {
    at: "2025-01-01T09:41:24.100Z",
    kind: "tool_result",
    call_id: "write-1",
    status: "ok",
    summary: "+128 −0 · new file\n\npub struct DeviceFlow {",
  },
  {
    at: "2025-01-01T09:41:24.500Z",
    kind: "permission_request",
    request_id: "perm-1",
    title: "cargo test -p grove-auth oauth -- --nocapture",
    detail: "Runs the auth crate's test suite. Writes to target/ and reads ~/.cargo.",
  },
];

const detail: SessionDetail = { ...session, events };

const plan: Plan = {
  sessionId: session.id,
  markdown: [
    "# Plan",
    "",
    "- [x] Harness registry and adapter trait",
    "- [x] Codex adapter: exec streaming",
    "- [x] ACP client handshake",
    "- [x] Session tabs with per-harness state",
    "- [ ] Permission gate inside the transcript",
    "- [ ] Read-only diff inspector",
    "- [ ] Persist session state across restart",
  ].join("\n"),
  version: 3,
  mirrorPath: "C:/code/grove/.grove/sessions/session-1/plan.md",
};

const harnesses: HarnessInfo[] = [
  {
    id: "claude",
    capabilities: {
      streamingDeltas: true,
      toolCalls: true,
      diffs: true,
      permissions: true,
      resume: true,
      images: false,
      mcp: true,
      usage: true,
    },
  },
  {
    id: "codex",
    capabilities: {
      streamingDeltas: true,
      toolCalls: true,
      diffs: true,
      permissions: true,
      resume: true,
      images: false,
      mcp: true,
      usage: true,
    },
  },
];

const specGraph: SpecGraph = {
  root: project.path,
  nodes: [
    {
      id: "0007-harness-registry",
      type: "architecture-design",
      title: "0007-harness-registry",
      status: "Accepted",
      path: "specs/0007-harness-registry.md",
      dependsOn: [],
      references: [],
      implements: [],
      covers: [],
    },
    {
      id: "0009-permission-gate",
      type: "module-design",
      title: "0009-permission-gate",
      status: "Draft",
      path: "specs/0009-permission-gate.md",
      dependsOn: [],
      references: [],
      implements: [],
      covers: [],
    },
    {
      id: "0011-diff-inspector",
      type: "module-design",
      title: "0011-diff-inspector",
      status: "Draft",
      path: "specs/0011-diff-inspector.md",
      dependsOn: [],
      references: [],
      implements: [],
      covers: [],
    },
  ],
  validation: { danglingLinks: [], duplicateIds: [], parentCycles: [] },
};

const skillCatalog: SkillCatalog = {
  entries: [
    {
      id: "anthropics/skills::skills/harness-adapter",
      name: "harness-adapter",
      source: "anthropics/skills",
      path: "skills/harness-adapter",
    },
    {
      id: "anthropics/skills::skills/pr-notes",
      name: "pr-notes",
      source: "anthropics/skills",
      path: "skills/pr-notes",
    },
    {
      id: "mattpocock/skills::skills/spec-writer",
      name: "spec-writer",
      source: "mattpocock/skills",
      path: "skills/spec-writer",
    },
  ],
  fetchedAt: "2025-01-01T00:00:00Z",
  fromCache: false,
  offline: false,
};

const doctor: DoctorReport = {
  harnesses: [
    {
      id: "claude",
      label: "Claude Code",
      installed: true,
      version: "1.0.80",
      path: "C:/Tools/claude.exe",
      detail: null,
    },
    {
      id: "codex",
      label: "Codex",
      installed: true,
      version: "0.9.2",
      path: "C:/Tools/codex.exe",
      detail: null,
    },
  ],
  tools: [],
  checkedAt: "2025-01-01T00:00:00.000Z",
};

const mcp: McpDiscovery = {
  servers: [
    {
      name: "filesystem",
      source: ".mcp.json",
      sources: [".mcp.json"],
      transport: "stdio",
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-filesystem", "."],
      url: null,
    },
    {
      name: "linear",
      source: "~/.claude.json",
      sources: ["~/.claude.json"],
      transport: "http",
      command: null,
      args: [],
      url: "https://mcp.linear.app/sse",
    },
  ],
  warnings: [],
};

const fileStats = [
  ["apps/desktop/src-tauri/src/session.rs", 42, 8],
  ["crates/agent-hub/src/harness/codex.rs", 18, 3],
  ["apps/desktop/src/features/session/SessionTabs.tsx", 33, 0],
  ["docs/harnesses.md", 9, 2],
  ["Cargo.toml", 2, 1],
] as const;

function fakeGitStatus() {
  return {
    repoRoot: worktree.path,
    branch: branch.name,
    head: worktree.head,
    clean: false,
    truncated: false,
    files: fileStats.map(([file, added]) => ({
      path: file,
      from: null,
      index: "M",
      worktree: added ? " " : "M",
      staged: false,
      untracked: false,
    })),
  };
}

function seedBridge() {
  const bridge = createFakeBridge(
    {},
    {
      harnesses,
      sessions: [fakeSummary({ ...session, awaiting: true }, events.length)],
      details: { [session.id]: detail },
      plans: { [session.id]: plan },
      projects: [project],
      worktrees: [worktree],
      branches: [branch],
      gitStatus: fakeGitStatus(),
      specGraph,
      specTitles: {
        "0007-harness-registry": "0007-harness-registry",
        "0009-permission-gate": "0009-permission-gate",
      },
      skillCatalog,
      skillTargets: [
        {
          id: "agents",
          label: "Project .agents/skills",
          path: `${project.path}/.agents/skills`,
          exists: true,
        },
      ],
      installs: [
        {
          skill: "pr-notes",
          source: "anthropics/skills",
          target: "agents",
          path: `${project.path}/.agents/skills/pr-notes`,
          installedAt: "2025-01-01T00:00:00Z",
        },
      ],
      doctor,
    },
  );
  bridge.on("git_diff", ({ path: filePath }) => {
    const selected = filePath ? fileStats.filter(([file]) => file === filePath) : [...fileStats];
    const patch = selected
      .map(([file, added, removed]) =>
        [
          `diff --git a/${file} b/${file}`,
          `--- a/${file}`,
          `+++ b/${file}`,
          "@@ -1,1 +1,1 @@",
          ...Array.from({ length: added }, (_, index) => `+line ${index}`),
          ...Array.from({ length: removed }, (_, index) => `-line ${index}`),
        ].join("\n"),
      )
      .join("\n");
    return {
      repoRoot: worktree.path,
      target: "working_tree",
      path: filePath ?? null,
      base: null,
      branch: null,
      patch,
      truncated: false,
    };
  });
  bridge.on("skill_preview", ({ source, path: skillPath }) => ({
    entry: skillCatalog.entries.find((entry) => entry.path === skillPath && entry.source === source) ?? skillCatalog.entries[0],
    frontmatter: {
      description: "Contract checklist for a new agent harness adapter: spawn, parse, interrupt.",
      version: "1.1.2",
      author: "Grove team",
      tags: "harness,acp",
    },
    body: "# Harness adapter\n\n- Walks spawn, parse and interrupt in order\n- Catches missing graceful shutdown\n- Lists the frames an adapter must emit",
  }));
  bridge.on("mcp_servers", () => mcp);
  return bridge;
}

async function renderReady() {
  const bridge = seedBridge();
  render(
    <BridgeProvider bridge={bridge}>
      <App />
    </BridgeProvider>,
  );
  await screen.findByRole("heading", { name: "OAuth device flow" });
  await waitFor(() => expect(screen.getAllByText(/1 awaiting you/).length).toBeGreaterThan(0));
  return bridge;
}

/** Serialize form-control state too; innerHTML alone loses `value`/`checked`/`selected`. */
function flattenTranscript(): void {
  // jsdom reports zero-height rows, so the virtualizer stacks every row at 0. The snapshot is a static
  // image of the rendered transcript: put the measured wrappers back into normal flow.
  const rows = document.querySelector(".run-rows");
  if (!rows) return;
  for (const row of Array.from(rows.querySelectorAll<HTMLElement>(".run-row"))) {
    row.style.position = "static";
    row.style.transform = "none";
  }
  (rows as HTMLElement).style.height = "auto";
}

function serializeBody(): string {
  flattenTranscript();
  for (const input of Array.from(document.querySelectorAll("input"))) {
    if (input.type === "checkbox" || input.type === "radio") {
      if (input.checked) input.setAttribute("checked", "");
      else input.removeAttribute("checked");
    } else {
      input.setAttribute("value", input.value);
    }
  }
  for (const textarea of Array.from(document.querySelectorAll("textarea"))) {
    textarea.textContent = textarea.value;
  }
  for (const select of Array.from(document.querySelectorAll("select"))) {
    for (const option of Array.from(select.options)) {
      if (option.selected) option.setAttribute("selected", "");
      else option.removeAttribute("selected");
    }
  }
  return document.body.innerHTML;
}

function writeSnapshot(name: string) {
  const dist = path.resolve(process.cwd(), "dist");
  const css = readdirSync(path.join(dist, "assets")).find(
    (file) => file.startsWith("index-") && file.endsWith(".css"),
  );
  const html = `<!doctype html><html lang="en" data-theme="${document.documentElement.dataset.theme ?? "house"}" data-mode="${document.documentElement.dataset.mode ?? "dark"}"><head><meta charset="utf-8"><link rel="stylesheet" href="/assets/${css}"></head><body>${serializeBody()}<script>window.addEventListener("load",function(){var r=document.querySelector(".run-scroll");if(r)r.scrollTop=r.scrollHeight;});</script></body></html>`;
  writeFileSync(path.join(dist, `${name}.html`), html);
}

beforeEach(() => {
  localStorage.clear();
  useSessionStore.getState().reset();
  useWorkspaceStore.getState().reset();
  useContextStore.getState().reset();
  useUiStore.getState().reset();
});

describe("visual snapshots", () => {
  it("default shell", async () => {
    await renderReady();
    await act(async () => {});
    writeSnapshot("visual-default");
  });

  it("git context", async () => {
    await renderReady();
    act(() => useUiStore.getState().setContextTab("git"));
    await act(async () => {});
    writeSnapshot("visual-git");
  });

  it("plan context", async () => {
    await renderReady();
    act(() => useUiStore.getState().setContextTab("plan"));
    await act(async () => {});
    writeSnapshot("visual-plan");
  });

  it("spec context", async () => {
    await renderReady();
    act(() => useUiStore.getState().setContextTab("spec"));
    await waitFor(() => expect(screen.getByText("0007-harness-registry")).toBeInTheDocument());
    writeSnapshot("visual-spec");
  });

  it("settings theme", async () => {
    await renderReady();
    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    await userEvent.click(screen.getByRole("tab", { name: /Theme/ }));
    await act(async () => {});
    writeSnapshot("visual-settings");
  });

  it("settings theme light", async () => {
    await renderReady();
    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    await userEvent.click(screen.getByRole("tab", { name: /Theme/ }));
    await userEvent.click(screen.getByRole("button", { name: "Light" }));
    await act(async () => {});
    writeSnapshot("visual-settings-light");
  });

  it("settings skills", async () => {
    await renderReady();
    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    await userEvent.click(screen.getByRole("tab", { name: /Skills/ }));
    await waitFor(() => expect(screen.getAllByText("harness-adapter").length).toBeGreaterThan(0));
    await act(async () => {});
    writeSnapshot("visual-skills");
  });

  it("settings harnesses", async () => {
    await renderReady();
    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    await userEvent.click(screen.getByRole("tab", { name: /Harnesses/ }));
    await act(async () => {});
    writeSnapshot("visual-harnesses");
  });

  it("settings mcp", async () => {
    await renderReady();
    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    await userEvent.click(screen.getByRole("tab", { name: /MCP/ }));
    await waitFor(() => expect(screen.getByText("filesystem")).toBeInTheDocument());
    await act(async () => {});
    writeSnapshot("visual-mcp");
  });

  it("new session dialog from a project", async () => {
    await renderReady();
    await userEvent.click(screen.getByRole("button", { name: "New session in grove" }));
    await act(async () => {});
    writeSnapshot("visual-new");
  });

  it("add project dialog", async () => {
    await renderReady();
    await userEvent.click(screen.getByRole("button", { name: /Add project/ }));
    await act(async () => {});
    writeSnapshot("visual-add");
  });

  it("diff modal", async () => {
    await renderReady();
    act(() => useUiStore.getState().openDiff("apps/desktop/src-tauri/src/session.rs"));
    await waitFor(() => expect(screen.getByText("Diff")).toBeInTheDocument());
    await act(async () => {});
    writeSnapshot("visual-diff");
  });
});
