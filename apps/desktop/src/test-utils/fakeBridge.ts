import type {
  Decision,
  ProviderStatus,
  EventEnvelope,
  GitBranch,
  GitStatus,
  GitWorktree,
  QuestionAnswer,
  SessionEvent,
  ProviderSpec,
  Plan,
  PlanUpdate,
  Project,
  SecretStatus,
  Session,
  SessionContext,
  SessionDetail,
  SessionSummary,
  Settings,
  SettingsPatch,
  SkillCatalog,
  SkillTarget,
  SpecGraph,
  ThinkingLevel,
} from "@grove/contract";
import type { GroveBridge, GroveEventName, UnlistenFn } from "../ipc/bridge";
import type { CommandArgs, CommandMap, CommandName } from "../ipc/commands";

export interface FakeBridgeCall {
  command: string;
  args: unknown;
}

/** What the fake core already knows when a test renders. */
export interface FakeBridgeSeed {
  providers?: ProviderSpec[];
  sessions?: SessionSummary[];
  details?: Record<string, SessionDetail>;
  plans?: Record<string, Plan>;
  settings?: Settings;
  /** Secret key → status. A key that is absent from the map reports `absent`. */
  secretStatuses?: Record<string, SecretStatus>;
  /** Provider readiness rows the settings pane renders. */
  providerStatuses?: ProviderStatus[];
  /** Provider → model → reasoning levels, as the cached models.dev catalogue would return. */
  modelThinking?: Record<string, Record<string, ThinkingLevel[]>>;
  /** Session id → pinned spec ids, in pin order. */
  pins?: Record<string, string[]>;
  /** Spec id → title, so pinned specs read like the Specs panel shows them. */
  specTitles?: Record<string, string>;
  /** The project root the fake core starts at; `app_set_project_root` moves it. */
  projectRoot?: string;
  projects?: Project[];
  worktrees?: GitWorktree[];
  branches?: GitBranch[];
  gitStatus?: GitStatus;
  specGraph?: SpecGraph;
  skillCatalog?: SkillCatalog;
  skillTargets?: SkillTarget[];
  /** Installed skill records the fake core remembers. */
  installs?: Array<{
    skill: string;
    source: string;
    target: string;
    path: string;
    installedAt: string;
  }>;
}

export interface FakeBridge extends GroveBridge {
  /** Deliver a raw payload to every listener of an event, exactly as Tauri would. */
  emit(event: GroveEventName, payload: unknown): void;
  /** Emit a well-formed session event envelope to the session stream. */
  emitSessionEvent(envelope: EventEnvelope): void;
  listenerCount(event: GroveEventName): number;
  /** Register a handler for one command; replaces the built-in defaults for that command. */
  on<K extends CommandName>(
    command: K,
    handler: (args: CommandArgs<K>) => CommandMap[K]["result"] | Promise<CommandMap[K]["result"]>,
  ): void;
  invocations: string[];
  /** Every invocation with its arguments, for asserting what the UI asked the core to do. */
  calls: FakeBridgeCall[];
  /** Replace a session's plan as an agent edit would: the next `plan_get` sees the new version. */
  setPlan(plan: Plan): void;
  /** The pins the fake core currently holds for one session. */
  pinsOf(sessionId: string): string[];
  /** The project root the fake core is looking at. */
  projectRoot(): string;
}

const DEFAULT_APP_INFO = {
  name: "Grove",
  version: "0.1.0",
  appDataDir: "C:/tmp/Grove",
  platform: "windows",
} satisfies CommandMap["app_info"]["result"];

export const MOCK_CAPABILITIES = {
  streamingDeltas: true,
  toolCalls: true,
  diffs: false,
  permissions: false,
  resume: true,
  images: false,
  mcp: false,
  usage: true,
} satisfies ProviderSpec["capabilities"];

export const DEFAULT_PROVIDERS: ProviderSpec[] = [
  {
    id: "mock",
    name: "Offline",
    short: "OF",
    tint: "#8a8f98",
    wire: "open_ai",
    baseUrl: "offline://mock",
    auth: "api_key",
    keyUrl: "https://example.invalid",
    envVar: "GROVE_OFFLINE_PROVIDER",
    models: ["mock-1"],
    defaultModel: "mock-1",
    capabilities: MOCK_CAPABILITIES,
  },
];

export const DEFAULT_SETTINGS: Settings = {
  catalogSources: ["anthropics/skills", "mattpocock/skills"],
  theme: "system",
};

/** A provider row for a UI test: Anthropic with a keychain credential unless overridden. */
export function fakeProviderStatus(overrides: Partial<ProviderStatus> = {}): ProviderStatus {
  return {
    id: "anthropic",
    name: "Anthropic",
    short: "AN",
    tint: "#c08a63",
    method: "api_key",
    keyUrl: "https://console.anthropic.com/settings/keys",
    envVar: "ANTHROPIC_API_KEY",
    defaultModel: "claude-sonnet-4-5",
    models: ["claude-sonnet-4-5", "claude-opus-4-1", "claude-haiku-4-5"],
    configured: true,
    source: "keychain",
    detail: null,
    ...overrides,
  };
}

/** A representative provider roster for the settings tests and snapshots. */
export const FAKE_PROVIDER_ROSTER: ProviderStatus[] = [
  fakeProviderStatus(),
  fakeProviderStatus({
    id: "openai",
    name: "OpenAI",
    short: "OA",
    tint: "#6f9ad1",
    keyUrl: "https://platform.openai.com/api-keys",
    envVar: "OPENAI_API_KEY",
    defaultModel: "gpt-5",
    models: ["gpt-5", "gpt-5-mini"],
    source: "environment",
  }),
  fakeProviderStatus({
    id: "google",
    name: "Google",
    short: "GO",
    tint: "#6fbf9a",
    keyUrl: "https://aistudio.google.com/apikey",
    envVar: "GEMINI_API_KEY",
    defaultModel: "gemini-2.5-pro",
    models: ["gemini-2.5-pro", "gemini-2.5-flash"],
    configured: false,
    source: null,
    detail: "Add an API key from https://aistudio.google.com/apikey, or set GEMINI_API_KEY.",
  }),
  fakeProviderStatus({
    id: "openrouter",
    name: "OpenRouter",
    short: "OR",
    tint: "#9d86c9",
    keyUrl: "https://openrouter.ai/keys",
    envVar: "OPENROUTER_API_KEY",
    defaultModel: "anthropic/claude-sonnet-4.5",
    models: ["anthropic/claude-sonnet-4.5", "openai/gpt-5"],
    configured: false,
    source: null,
    detail: "Add an API key from https://openrouter.ai/keys, or set OPENROUTER_API_KEY.",
  }),
];

/** What the Providers pane renders: one row per shipped vendor, none configured by default. */
export const DEFAULT_PROVIDER_STATUSES: ProviderStatus[] = [
  fakeProviderStatus({
    configured: false,
    source: null,
    detail:
      "Add an API key from https://console.anthropic.com/settings/keys, or set ANTHROPIC_API_KEY.",
  }),
];

export function fakeSession(overrides: Partial<Session> = {}): Session {
  return {
    id: "session-1",
    provider: "mock",
    cwd: "C:/projects/grove",
    model: null,
    title: "mock session",
    status: "idle",
    projectId: null,
    awaiting: false,
    createdAt: "2025-01-01T00:00:00.000Z",
    updatedAt: "2025-01-01T00:00:00.000Z",
    ...overrides,
  };
}

export function fakeSummary(session: Session = fakeSession(), eventCount = 0): SessionSummary {
  return { ...session, eventCount };
}

/**
 * The bridge every UI test renders under. Unknown commands fail loudly, so a panel that reaches for
 * UI-level Tauri instead of the bridge is caught in test rather than at runtime.
 *
 * Session commands are backed by a small in-memory core: `session_create` really adds a session and
 * `session_get` really returns its detail, so a panel can be driven like the app without a real store.
 * Live events still have to be pushed with `emitSessionEvent`, exactly like Tauri delivers them.
 * A command registered with `on()` wins over every default below.
 */
export function createFakeBridge(
  overrides: Partial<GroveBridge> = {},
  seed: FakeBridgeSeed = {},
): FakeBridge {
  const handlers = new Map<string, Set<(payload: unknown) => void>>();
  const commands = new Map<string, (args: unknown) => unknown>();
  const invocations: string[] = [];
  const calls: FakeBridgeCall[] = [];
  const providers = seed.providers ?? DEFAULT_PROVIDERS;
  // Deep clone the seed: a test's fixtures must survive a bridge that mutates its own rows, exactly
  // like the real core does not mutate the caller's objects.
  const sessions: SessionSummary[] = structuredClone(seed.sessions ?? []);
  const details: Record<string, SessionDetail> = structuredClone(seed.details ?? {});
  const plans: Record<string, Plan> = structuredClone(seed.plans ?? {});
  const pins: Record<string, string[]> = Object.fromEntries(
    Object.entries(seed.pins ?? {}).map(([sessionId, specIds]) => [sessionId, [...specIds]]),
  );
  const specTitles = seed.specTitles ?? {};

  const emptyPlan = (sessionId: string): Plan => ({
    sessionId,
    markdown: "",
    version: 0,
    mirrorPath: `C:/projects/grove/.grove/sessions/${sessionId}/plan.md`,
  });
  const settings: Settings = structuredClone(seed.settings ?? DEFAULT_SETTINGS);
  /** The project root the fake core is looking at (`app_set_project_root` moves it). */
  let projectRoot = seed.projectRoot ?? "C:/projects/grove";
  let windowMaximized = false;
  const secretStatuses = new Map<string, SecretStatus>(
    Object.entries(seed.secretStatuses ?? {}),
  );
  const providerStatuses = () => structuredClone(seed.providerStatuses ?? DEFAULT_PROVIDER_STATUSES);
  const projects: Project[] = structuredClone(seed.projects ?? []);
  const worktrees = structuredClone(seed.worktrees ?? []);
  const branches = structuredClone(seed.branches ?? []);
  const installs = structuredClone(seed.installs ?? []);
  const defaultGit = (): GitStatus => ({
    repoRoot: projectRoot,
    branch: "main",
    head: null,
    clean: true,
    truncated: false,
    files: [],
  });
  let gitStatus: GitStatus = structuredClone(seed.gitStatus ?? defaultGit());

  const setStatus = (id: string, status: Session["status"]) => {
    const row = sessions.find((session) => session.id === id);
    if (row) row.status = status;
    const detail = details[id];
    if (detail) detail.status = status;
  };

  /** The snapshot the core would assemble: pinned specs in pin order, then the plan. */
  const contextOf = (sessionId: string): SessionContext => {
    const plan = plans[sessionId] ?? emptyPlan(sessionId);
    const pinned = pins[sessionId] ?? [];
    const documents = pinned.map((specId) => {
      const title = specTitles[specId] ?? specId;
      return { id: specId, title, chars: title.length, truncated: false };
    });
    if (plan.markdown.trim().length > 0) {
      documents.push({
        id: "plan",
        title: "Plan",
        chars: plan.markdown.length,
        truncated: false,
      });
    }
    return {
      sessionId,
      provider: details[sessionId]?.provider ?? sessions.find((row) => row.id === sessionId)?.provider ?? "mock",
      pins: pinned.map((specId) => ({ specId, title: specTitles[specId] ?? specId })),
      documents,
      totalChars: documents.reduce((total, doc) => total + doc.chars, 0),
      limits: { perDocChars: 8000, totalChars: 24000 },
      planVersion: plan.version,
    };
  };

  const documentsLabel = (count: number) => `${count} document${count === 1 ? "" : "s"}`;

  /** Append a `context_refreshed` note to the transcript and stream it, exactly like the core. */
  const appendContextNote = (
    sessionId: string,
    reason: "start" | "resume" | "refresh" | "provider_switch",
    note: string,
  ) => {
    const detail = details[sessionId];
    if (!detail) return;
    const context = contextOf(sessionId);
    const event: SessionEvent = {
      at: "2025-01-01T00:00:02.000Z",
      kind: "context_refreshed",
      reason,
      provider: detail.provider,
      documents: context.documents,
      note,
    };
    detail.events = [...detail.events, event];
    const row = sessions.find((session) => session.id === sessionId);
    if (row) row.eventCount = detail.events.length;
    bridge.emitSessionEvent({ sessionId, payload: event });
  };

  const bridge: FakeBridge = {
    async invoke<K extends CommandName>(
      command: K,
      args?: CommandArgs<K>,
    ): Promise<CommandMap[K]["result"]> {
      invocations.push(command);
      calls.push({ command, args });

      const handler = commands.get(command);
      if (handler) {
        return (await handler(args)) as CommandMap[K]["result"];
      }
      if (command === "app_info") {
        return DEFAULT_APP_INFO as CommandMap[K]["result"];
      }
      if (command === "app_set_project_root") {
        // The shell moves the core's root and announces it with `grove://spec-changed`, which is what
        // makes the Specs panel re-index; the fake does the same so a test sees the same chain.
        const { path } = args as { path: string };
        projectRoot = path;
        bridge.emit("grove://spec-changed", { payload: { paths: [] } });
        return path as CommandMap[K]["result"];
      }
      if (command === "app_window_minimize" || command === "app_window_close") {
        return null as CommandMap[K]["result"];
      }
      if (command === "app_window_is_maximized") {
        return windowMaximized as CommandMap[K]["result"];
      }
      if (command === "app_window_toggle_maximize") {
        windowMaximized = !windowMaximized;
        return windowMaximized as CommandMap[K]["result"];
      }
      if (command === "provider_list") {
        return providers as CommandMap[K]["result"];
      }
      if (command === "session_list") {
        return sessions.map((session) => ({ ...session })) as CommandMap[K]["result"];
      }
      if (command === "session_get") {
        const { id } = args as { id: string };
        const detail = details[id];
        if (!detail) throw new Error(`FakeBridge has no session: ${id}`);
        return structuredClone(detail) as CommandMap[K]["result"];
      }
      if (command === "session_create") {
        const { req } = args as {
          req: {
            provider: string;
            cwd?: string | null;
            model?: string | null;
            title?: string | null;
            projectId?: string | null;
            worktreePath?: string | null;
          };
        };
        const at = "2025-01-01T00:00:00.000Z";
        const project = req.projectId ? projects.find((row) => row.id === req.projectId) : null;
        const session = fakeSession({
          id: `session-${sessions.length + 1}`,
          provider: req.provider,
          cwd: req.worktreePath ?? project?.path ?? req.cwd ?? "C:/projects/grove",
          model: req.model ?? null,
          title: req.title ?? `${req.provider} session`,
          status: "starting",
          projectId: project?.id ?? null,
          createdAt: at,
          updatedAt: at,
        });
        const detail: SessionDetail = { ...session, events: [] };
        details[session.id] = detail;
        sessions.unshift({ ...session, eventCount: 0 });
        return detail as CommandMap[K]["result"];
      }
      if (command === "session_send") {
        return null as CommandMap[K]["result"];
      }
      if (command === "session_cancel") {
        return null as CommandMap[K]["result"];
      }
      if (command === "session_respond_permission") {
        // The real core answers the agent run and the runtime emits `permission_resolved`; the fake
        // does both, so a test sees exactly what the transcript sees.
        const { id, requestId, decision } = args as {
          id: string;
          requestId: string;
          decision: Decision;
        };
        const detail = details[id];
        if (!detail) throw new Error(`FakeBridge has no session: ${id}`);
        const event: SessionEvent = {
          at: "2025-01-01T00:00:04.000Z",
          kind: "permission_resolved",
          request_id: requestId,
          decision,
        };
        detail.events = [...detail.events, event];
        detail.awaiting = false;
        const row = sessions.find((session) => session.id === id);
        if (row) row.awaiting = false;
        bridge.emitSessionEvent({ sessionId: id, payload: event });
        return null as CommandMap[K]["result"];
      }
      if (command === "session_respond_question") {
        const { id, requestId, answers } = args as {
          id: string;
          requestId: string;
          answers: QuestionAnswer[];
        };
        const detail = details[id];
        if (!detail) throw new Error(`FakeBridge has no session: ${id}`);
        const event: SessionEvent = {
          at: "2025-01-01T00:00:04.100Z",
          kind: "question_resolved",
          request_id: requestId,
          answers,
        };
        detail.events = [...detail.events, event];
        detail.awaiting = false;
        const row = sessions.find((session) => session.id === id);
        if (row) row.awaiting = false;
        bridge.emitSessionEvent({ sessionId: id, payload: event });
        return null as CommandMap[K]["result"];
      }
      if (command === "session_set_thinking") {
        const { id, level } = args as { id: string; level: SessionDetail["thinkingLevel"] };
        const detail = details[id];
        if (!detail) throw new Error(`FakeBridge has no session: ${id}`);
        detail.thinkingLevel = level;
        return detail as CommandMap[K]["result"];
      }
      if (command === "session_close") {
        setStatus((args as { id: string }).id, "exited");
        return null as CommandMap[K]["result"];
      }
      if (command === "session_resume") {
        const { id } = args as { id: string };
        setStatus(id, "starting");
        const row = sessions.find((session) => session.id === id);
        if (!row) throw new Error(`FakeBridge has no session: ${id}`);
        const context = contextOf(id);
        if (context.documents.length > 0) {
          appendContextNote(
            id,
            "resume",
            `Context re-sent on resume to ${row.provider}: ${documentsLabel(context.documents.length)}`,
          );
        }
        return { ...row } as CommandMap[K]["result"];
      }
      if (command === "session_context_get") {
        const { id } = args as { id: string };
        if (!details[id]) throw new Error(`FakeBridge has no session: ${id}`);
        return contextOf(id) as CommandMap[K]["result"];
      }
      if (command === "session_pin") {
        const { id, specId, pinned } = args as { id: string; specId: string; pinned: boolean };
        if (!details[id]) throw new Error(`FakeBridge has no session: ${id}`);
        const current = pins[id] ?? [];
        pins[id] = pinned
          ? current.includes(specId)
            ? current
            : [...current, specId]
          : current.filter((candidate) => candidate !== specId);
        return contextOf(id) as CommandMap[K]["result"];
      }
      if (command === "session_refresh_context") {
        const { id } = args as { id: string };
        if (!details[id]) throw new Error(`FakeBridge has no session: ${id}`);
        const context = contextOf(id);
        appendContextNote(
          id,
          "refresh",
          `Context refreshed for ${context.provider}: ${documentsLabel(context.documents.length)} (${context.totalChars} chars)`,
        );
        return contextOf(id) as CommandMap[K]["result"];
      }
      if (command === "session_switch_provider") {
        const { id, provider, model } = args as {
          id: string;
          provider: string;
          model?: string;
        };
        const detail = details[id];
        const row = sessions.find((session) => session.id === id);
        if (!detail || !row) throw new Error(`FakeBridge has no session: ${id}`);
        const from = detail.provider;
        detail.provider = provider;
        row.provider = provider;
        if (model !== undefined) {
          detail.model = model;
          row.model = model;
        } else if (provider !== from) {
          detail.model = null;
          row.model = null;
        }
        detail.status = "starting";
        row.status = "starting";
        const started: SessionEvent = {
          at: "2025-01-01T00:00:03.000Z",
          kind: "session_started",
          provider,
          model: detail.model,
          resumed: true,
        };
        detail.events = [...detail.events, started];
        bridge.emitSessionEvent({ sessionId: id, payload: started });
        appendContextNote(
          id,
          "provider_switch",
          `Provider switched from ${from} to ${provider}; context re-sent: ${documentsLabel(contextOf(id).documents.length)}`,
        );
        return structuredClone(detail) as CommandMap[K]["result"];
      }
      if (command === "project_list") {
        return structuredClone(projects) as CommandMap[K]["result"];
      }
      if (command === "project_detect") {
        const { path } = args as { path: string };
        return {
          path,
          isGit: false,
          currentBranch: null,
          branches: [],
          defaultBranch: null,
          remote: null,
        } as CommandMap[K]["result"];
      }
      if (command === "project_add") {
        const { req } = args as {
          req: {
            path: string;
            name?: string | null;
            defaultBranch?: string | null;
            remote?: string | null;
          };
        };
        const name =
          req.name?.trim() || req.path.replace(/\\/g, "/").split("/").pop() || "project";
        const project: Project = {
          id: `project-${projects.length + 1}`,
          name,
          path: req.path,
          remote: req.remote ?? null,
          defaultBranch: req.defaultBranch ?? null,
          isGit: Boolean(req.defaultBranch || req.remote),
          createdAt: "2025-01-01T00:00:00.000Z",
          updatedAt: "2025-01-01T00:00:00.000Z",
        };
        projects.push(project);
        return structuredClone(project) as CommandMap[K]["result"];
      }
      if (command === "fs_list_dirs") {
        const { path } = (args ?? {}) as { path?: string };
        return {
          path: path ?? "C:/Users/demo",
          parent: null,
          dirs: [],
        } as CommandMap[K]["result"];
      }
      if (command === "git_status") {
        return structuredClone(gitStatus) as CommandMap[K]["result"];
      }
      if (command === "git_branches") {
        return structuredClone(branches) as CommandMap[K]["result"];
      }
      if (command === "git_worktrees") {
        return structuredClone(worktrees) as CommandMap[K]["result"];
      }
      if (command === "git_diff") {
        const { path } = args as { path?: string };
        return {
          repoRoot: projectRoot,
          target: "working_tree",
          path: path ?? null,
          base: null,
          branch: null,
          patch: "",
          truncated: false,
        } as CommandMap[K]["result"];
      }
      if (command === "git_stage") {
        const { paths, staged } = args as { paths: string[]; staged: boolean };
        gitStatus = {
          ...gitStatus,
          files: gitStatus.files.map((file) =>
            paths.includes(file.path) ? { ...file, staged } : file,
          ),
        };
        return structuredClone(gitStatus) as CommandMap[K]["result"];
      }
      if (command === "git_commit") {
        gitStatus = { ...gitStatus, clean: true, files: [] };
        return structuredClone(gitStatus) as CommandMap[K]["result"];
      }
      if (command === "git_open_pr") {
        const { draft, base } = args as { draft?: boolean; base?: string };
        return {
          url: "https://github.com/acme/grove/pull/1",
          draft: draft ?? false,
          base: base ?? null,
        } as CommandMap[K]["result"];
      }
      if (command === "git_worktree_create") {
        const { branch, path } = args as { branch: string; path: string };
        const worktree: GitWorktree = {
          path,
          head: null,
          branch,
          bare: false,
          detached: false,
          current: false,
        };
        worktrees.push(worktree);
        return structuredClone(worktree) as CommandMap[K]["result"];
      }
      if (command === "spec_index") {
        return (
          seed.specGraph
            ? structuredClone(seed.specGraph)
            : {
                root: projectRoot,
                nodes: [],
                validation: { danglingLinks: [], duplicateIds: [], parentCycles: [] },
              }
        ) as CommandMap[K]["result"];
      }
      if (command === "spec_read") {
        const { path } = args as { path: string };
        return {
          node: {
            id: path.replace(/\.md$/, ""),
            type: "module-design",
            title: path,
            status: "accepted",
            path,
            dependsOn: [],
            references: [],
            implements: [],
            covers: [],
          },
          body: "# Spec\n\nBody",
          hash: "hash",
        } as CommandMap[K]["result"];
      }
      if (command === "skill_catalog") {
        return (
          seed.skillCatalog ?? {
            entries: [],
            fetchedAt: null,
            fromCache: false,
            offline: false,
          }
        ) as CommandMap[K]["result"];
      }
      if (command === "skill_preview") {
        const { source, path } = args as { source: string; path: string };
        return {
          entry: {
            id: `${source}::${path}`,
            name: path.replace(/\\/g, "/").split("/").pop() ?? path,
            source,
            path,
          },
          frontmatter: { description: "Fake skill" },
          body: "# Fake skill",
        } as CommandMap[K]["result"];
      }
      if (command === "skill_targets") {
        return structuredClone(seed.skillTargets ?? []) as CommandMap[K]["result"];
      }
      if (command === "skill_install") {
        const { source, path, target } = args as {
          source: string;
          path: string;
          target: string;
        };
        const skill = path.replace(/\\/g, "/").split("/").pop() ?? path;
        const result = {
          skill,
          source,
          target,
          path: `C:/projects/grove/.agents/skills/${skill}`,
          overwritten: false,
          installedAt: "2025-01-01T00:00:00Z",
        };
        installs.push({
          skill,
          source,
          target,
          path: result.path,
          installedAt: result.installedAt,
        });
        return result as CommandMap[K]["result"];
      }
      if (command === "skill_installs") {
        return structuredClone(installs) as CommandMap[K]["result"];
      }
      if (command === "skill_remove") {
        const { path } = args as { path: string };
        const index = installs.findIndex((record) => record.path === path);
        if (index >= 0) installs.splice(index, 1);
        return null as CommandMap[K]["result"];
      }
      if (command === "mcp_servers") {
        return { servers: [], warnings: [] } as CommandMap[K]["result"];
      }
      if (command === "mcp_probe") {
        const { name, source } = args as { name: string; source: string };
        return {
          name,
          source,
          transport: "stdio",
          state: "connected",
          tools: [],
          error: null,
          stderrTail: null,
        } as CommandMap[K]["result"];
      }
      if (command === "plan_get") {
        const { id } = args as { id: string };
        return { ...(plans[id] ?? emptyPlan(id)) } as CommandMap[K]["result"];
      }
      if (command === "plan_set") {
        const { id, update } = args as { id: string; update: PlanUpdate };
        const current = plans[id] ?? emptyPlan(id);
        if (current.version !== update.expectedVersion) {
          throw {
            code: "conflict",
            message: `plan changed: expected version ${update.expectedVersion}, found ${current.version}`,
            detail: current.mirrorPath,
          };
        }
        const next: Plan = {
          ...current,
          markdown: update.markdown,
          version: current.version + 1,
        };
        plans[id] = next;
        return { ...next } as CommandMap[K]["result"];
      }
      if (command === "settings_get") {
        return structuredClone(settings) as CommandMap[K]["result"];
      }
      if (command === "settings_set") {
        const { patch } = args as { patch: SettingsPatch };
        const { catalogSources, theme, ...extras } = patch;
        if (catalogSources) settings.catalogSources = [...catalogSources];
        if (theme) settings.theme = theme;
        Object.assign(settings, extras);
        return structuredClone(settings) as CommandMap[K]["result"];
      }
      if (command === "secret_status") {
        const { key } = args as { key: string };
        return {
          key,
          status: secretStatuses.get(key) ?? "absent",
        } as CommandMap[K]["result"];
      }
      if (command === "secret_set") {
        const { key } = args as { key: string; value: string };
        secretStatuses.set(key, "present");
        return { key, status: "present" } as CommandMap[K]["result"];
      }
      if (command === "secret_delete") {
        const { key } = args as { key: string };
        secretStatuses.set(key, "absent");
        return { key, status: "absent" } as CommandMap[K]["result"];
      }
      if (command === "provider_statuses") {
        return providerStatuses() as CommandMap[K]["result"];
      }
      if (command === "provider_key_set") {
        const { provider } = args as { provider: string; key: string };
        const rows = providerStatuses().map((row) =>
          row.id === provider
            ? { ...row, configured: true, source: "keychain" as const, detail: null }
            : row,
        );
        return rows.find((row) => row.id === provider) as CommandMap[K]["result"];
      }
      if (command === "provider_key_delete") {
        const { provider } = args as { provider: string };
        const rows = providerStatuses().map((row) =>
          row.id === provider
            ? { ...row, configured: false, source: null, detail: "No key configured." }
            : row,
        );
        return rows.find((row) => row.id === provider) as CommandMap[K]["result"];
      }
      if (command === "provider_models") {
        const { provider } = args as { provider: string };
        const row = providerStatuses().find((item) => item.id === provider);
        return (row?.models ?? []) as CommandMap[K]["result"];
      }
      if (command === "provider_thinking") {
        return structuredClone(seed.modelThinking ?? {}) as CommandMap[K]["result"];
      }

      throw new Error(`FakeBridge has no handler for command: ${command}`);
    },
    async listen(event: GroveEventName, handler: (payload: unknown) => void): Promise<UnlistenFn> {
      const set = handlers.get(event) ?? new Set<(payload: unknown) => void>();
      set.add(handler);
      handlers.set(event, set);
      return () => {
        set.delete(handler);
      };
    },
    emit(event, payload) {
      for (const handler of handlers.get(event) ?? []) handler(payload);
    },
    emitSessionEvent(envelope) {
      bridge.emit("grove://session-event", envelope);
    },
    listenerCount(event) {
      return handlers.get(event)?.size ?? 0;
    },
    on(command, handler) {
      commands.set(command, handler as (args: unknown) => unknown);
    },
    setPlan(plan) {
      plans[plan.sessionId] = { ...plan };
    },
    pinsOf(sessionId) {
      return [...(pins[sessionId] ?? [])];
    },
    projectRoot() {
      return projectRoot;
    },
    invocations,
    calls,
    ...overrides,
  };

  return bridge;
}
