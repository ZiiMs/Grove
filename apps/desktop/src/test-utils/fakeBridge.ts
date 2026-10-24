import type {
  DoctorReport,
  EventEnvelope,
  HarnessInfo,
  Plan,
  PlanUpdate,
  SecretStatus,
  Session,
  SessionDetail,
  SessionSummary,
  Settings,
  SettingsPatch,
} from "@grove/contract";
import type { GroveBridge, GroveEventName, UnlistenFn } from "../ipc/bridge";
import type { CommandArgs, CommandMap, CommandName } from "../ipc/commands";

export interface FakeBridgeCall {
  command: string;
  args: unknown;
}

/** What the fake core already knows when a test renders. */
export interface FakeBridgeSeed {
  harnesses?: HarnessInfo[];
  sessions?: SessionSummary[];
  details?: Record<string, SessionDetail>;
  plans?: Record<string, Plan>;
  settings?: Settings;
  /** Secret key → status. A key that is absent from the map reports `absent`. */
  secretStatuses?: Record<string, SecretStatus>;
  doctor?: DoctorReport;
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
} satisfies HarnessInfo["capabilities"];

export const DEFAULT_HARNESSES: HarnessInfo[] = [{ id: "mock", capabilities: MOCK_CAPABILITIES }];

export const DEFAULT_SETTINGS: Settings = {
  harnessPaths: {},
  catalogSources: ["anthropics/skills", "mattpocock/skills"],
  theme: "system",
};

export const DEFAULT_DOCTOR: DoctorReport = {
  harnesses: [
    {
      id: "mock",
      label: "Mock (builtin)",
      installed: true,
      version: "0.1.0",
      path: "builtin:mock",
      detail: null,
    },
  ],
  tools: [
    {
      id: "node",
      label: "Node.js",
      installed: true,
      version: "v20.11.1",
      path: "C:/Program Files/nodejs/node.exe",
      detail: null,
    },
    {
      id: "git",
      label: "Git",
      installed: true,
      version: "git version 2.43.0",
      path: "C:/Program Files/Git/cmd/git.exe",
      detail: null,
    },
    {
      id: "gh",
      label: "GitHub CLI",
      installed: false,
      version: null,
      path: null,
      detail: "gh is not installed or not on PATH",
    },
  ],
  checkedAt: "2025-01-01T00:00:00.000Z",
};

export function fakeSession(overrides: Partial<Session> = {}): Session {
  return {
    id: "session-1",
    harness: "mock",
    cwd: "C:/projects/grove",
    model: null,
    title: "mock session",
    status: "idle",
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
  const harnesses = seed.harnesses ?? DEFAULT_HARNESSES;
  const sessions: SessionSummary[] = [...(seed.sessions ?? [])];
  const details: Record<string, SessionDetail> = { ...(seed.details ?? {}) };
  const plans: Record<string, Plan> = { ...(seed.plans ?? {}) };

  const emptyPlan = (sessionId: string): Plan => ({
    sessionId,
    markdown: "",
    version: 0,
    mirrorPath: `C:/projects/grove/.grove/sessions/${sessionId}/plan.md`,
  });
  const settings: Settings = structuredClone(seed.settings ?? DEFAULT_SETTINGS);
  const secretStatuses = new Map<string, SecretStatus>(
    Object.entries(seed.secretStatuses ?? {}),
  );
  const doctor = () => structuredClone(seed.doctor ?? DEFAULT_DOCTOR);

  const setStatus = (id: string, status: Session["status"]) => {
    const row = sessions.find((session) => session.id === id);
    if (row) row.status = status;
    const detail = details[id];
    if (detail) detail.status = status;
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
      if (command === "harness_list") {
        return harnesses as CommandMap[K]["result"];
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
          req: { harness: string; cwd?: string | null; model?: string | null; title?: string | null };
        };
        const at = "2025-01-01T00:00:00.000Z";
        const session = fakeSession({
          id: `session-${sessions.length + 1}`,
          harness: req.harness,
          cwd: req.cwd ?? "C:/projects/grove",
          model: req.model ?? null,
          title: req.title ?? `${req.harness} session`,
          status: "starting",
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
      if (command === "session_close") {
        setStatus((args as { id: string }).id, "exited");
        return null as CommandMap[K]["result"];
      }
      if (command === "session_resume") {
        const { id } = args as { id: string };
        setStatus(id, "starting");
        const row = sessions.find((session) => session.id === id);
        if (!row) throw new Error(`FakeBridge has no session: ${id}`);
        return { ...row } as CommandMap[K]["result"];
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
        const { harnessPaths, catalogSources, theme, ...extras } = patch;
        if (harnessPaths) settings.harnessPaths = { ...harnessPaths };
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
      if (command === "doctor_report") {
        return doctor() as CommandMap[K]["result"];
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
    invocations,
    calls,
    ...overrides,
  };

  return bridge;
}
