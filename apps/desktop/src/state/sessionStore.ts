import type {
  Decision,
  SessionEvent,
  ProviderSpec,
  ProviderStatus,
  QuestionAnswer,
  ThinkingLevel,
  SessionContext,
  SessionDetail,
  SessionSummary,
} from "@grove/contract";
import { create } from "zustand";
import type { GroveBridge } from "../ipc/bridge";
import { errorText } from "../ipc/errors";

/**
 * Session domain state: the list, the open tabs, the active transcript, and the commands that move
 * them. Panels render from here and pass the bridge in, so the store never imports Tauri and every UI
 * test runs under `FakeBridge`.
 *
 * Tabs are ids, not components: `openIds` is what the user has open side by side, `activeId` is the one
 * whose transcript renders, and `drafts` keeps each tab's compose text so switching never loses typing.
 */

export interface CreateSessionInput {
  provider: string;
  cwd?: string | null;
  model?: string | null;
  title?: string | null;
  /** The workspace project the session belongs to; `cwd` then resolves from it. */
  projectId?: string | null;
  /** A worktree checkout to run in; only meaningful with `projectId`. */
  worktreePath?: string | null;
  /**
   * Provider config, passed straight through to the runtime. The documented key is `maxSteps`, the
   * tool-loop ceiling. No form field sets it yet, so it stays untyped here rather than inventing a
   * shape per provider. Omitted when undefined, so the request stays minimal.
   */
  config?: unknown;
}

interface SessionStoreData {
  providers: ProviderSpec[];
  /** Credential readiness for the registered providers; drives which models a session may use. */
  providerStatuses: ProviderStatus[];
  sessions: SessionSummary[];
  activeId: string | null;
  detail: SessionDetail | null;
  events: SessionEvent[];
  /** Session ids open as tabs, in the order they were opened. */
  openIds: string[];
  /** Compose text per session: a draft belongs to its tab, not to the panel. */
  drafts: Record<string, string>;
  /** Messages typed while a turn is running, sent one at a time as the session goes idle. */
  queues: Record<string, string[]>;
  /** The active session's knowledge (pins, plan, the documents a start would send). */
  context: SessionContext | null;
  loading: boolean;
  busy: boolean;
  error: string | null;
}

export interface SessionStore extends SessionStoreData {
  /** Provider catalogue and the session list. */
  load: (bridge: GroveBridge) => Promise<void>;
  /** Fetch the models each configured provider actually exposes and update the pickers. */
  refreshModels: (bridge: GroveBridge) => Promise<void>;
  create: (bridge: GroveBridge, input: CreateSessionInput) => Promise<boolean>;
  /** Open (or focus) a session as a tab. */
  select: (bridge: GroveBridge, id: string) => Promise<void>;
  /** Close a tab; the next open one becomes active. The session itself is untouched. */
  closeTab: (bridge: GroveBridge, id: string) => Promise<void>;
  setDraft: (id: string, text: string) => void;
  enqueue: (id: string, text: string) => void;
  dequeue: (id: string, index: number) => void;
  /** Send the first queued message if the active session is no longer busy. */
  flushQueue: (bridge: GroveBridge) => Promise<void>;
  /** Returns whether the message was accepted, so the panel knows when to clear the compose box. */
  send: (bridge: GroveBridge, text: string) => Promise<boolean>;
  cancel: (bridge: GroveBridge) => Promise<void>;
  /**
   * Answer a pending permission request for the active session. The transcript does not change here:
   * the buttons disappear when the agent's `permission_resolved` event arrives, so the UI shows
   * what the agent actually accepted.
   */
  respondPermission: (bridge: GroveBridge, requestId: string, decision: Decision) => Promise<void>;
  respondQuestion: (bridge: GroveBridge, requestId: string, answers: QuestionAnswer[]) => Promise<boolean>;
  setThinking: (bridge: GroveBridge, level: ThinkingLevel) => Promise<boolean>;
  /** Restart a stopped session; `send` calls this automatically before the next message. */
  resume: (bridge: GroveBridge) => Promise<boolean>;
  close: (bridge: GroveBridge) => Promise<void>;
  /** Adopt a snapshot another panel produced (the Specs panel pins and unpins). */
  applyContext: (context: SessionContext) => void;
  /** Re-send the current pins and plan to the running agent, then show the new snapshot. */
  /** Returns whether the running agent accepted the refreshed snapshot. */
  refreshContext: (bridge: GroveBridge) => Promise<boolean>;
  /** Re-launch the active session on another registered provider, keeping its transcript. */
  switchProvider: (bridge: GroveBridge, provider: string, model?: string) => Promise<void>;
  /** Fold one live `grove://session-event` into the list and, when it belongs to the open session, the transcript. */
  applyEvent: (sessionId: string | null, event: SessionEvent) => void;
  reset: () => void;
}

const initial: SessionStoreData = {
  providers: [],
  providerStatuses: [],
  sessions: [],
  activeId: null,
  detail: null,
  events: [],
  openIds: [],
  drafts: {},
  queues: {},
  context: null,
  loading: true,
  busy: false,
  error: null,
};


function summaryOf(detail: SessionDetail): SessionSummary {
  const { events, thinkingLevel: _thinkingLevel, ...session } = detail;
  return { ...session, eventCount: events.length };
}

/**
 * A session's working directory *is* the project: tell the core so the spec graph, the plan mirror,
 * MCP discovery and the project-local skills target follow the session the user is looking at instead
 * of the app's own directory. Best-effort: a core that refuses must not block opening a session.
 */
async function syncProjectRoot(bridge: GroveBridge, cwd: string): Promise<void> {
  const path = cwd.trim();
  if (!path) return;
  try {
    await bridge.invoke("app_set_project_root", { path });
  } catch {
    // The session still opens; the panels keep indexing whatever the core last accepted.
  }
}

/** The knowledge snapshot is a bonus for the header: a failure must not block opening a session. */
async function contextOf(bridge: GroveBridge, id: string): Promise<SessionContext | null> {
  try {
    return await bridge.invoke("session_context_get", { id });
  } catch {
    return null;
  }
}

/** Add an id to the open tabs without duplicating it. */
function opened(openIds: string[], id: string): string[] {
  return openIds.includes(id) ? openIds : [...openIds, id];
}

export const useSessionStore = create<SessionStore>((set, get) => ({
  ...initial,

  load: async (bridge) => {
    set({ loading: true, error: null });
    try {
      const [providers, providerStatuses, sessions] = await Promise.all([
        bridge.invoke("provider_list"),
        bridge.invoke("provider_statuses"),
        bridge.invoke("session_list"),
      ]);
      set({ providers, providerStatuses, sessions, loading: false });
      void get().refreshModels(bridge);
    } catch (error) {
      set({ loading: false, error: errorText(error) });
    }
  },

  refreshModels: async (bridge) => {
    const { providerStatuses } = get();
    const configured = providerStatuses.filter((status) => status.configured).map((status) => status.id);
    if (configured.length === 0) return;
    const results = await Promise.allSettled(
      configured.map(async (provider) => {
        const models = await bridge.invoke("provider_models", { provider });
        return [provider, models] as const;
      }),
    );
    const discovered = new Map<string, string[]>();
    for (const result of results) {
      if (result.status === "fulfilled" && result.value[1].length > 0) {
        discovered.set(result.value[0], result.value[1]);
      }
    }
    if (discovered.size === 0) return;
    set((state) => ({
      providers: state.providers.map((provider) => {
        const models = discovered.get(provider.id);
        if (!models) return provider;
        return {
          ...provider,
          models,
          defaultModel: models.includes(provider.defaultModel) ? provider.defaultModel : models[0],
        };
      }),
      providerStatuses: state.providerStatuses.map((status) => {
        const models = discovered.get(status.id);
        if (!models) return status;
        return {
          ...status,
          models,
          defaultModel: models.includes(status.defaultModel) ? status.defaultModel : models[0],
        };
      }),
    }));
  },

  create: async (bridge, input) => {
    set({ busy: true, error: null });
    try {
      const { config, ...rest } = input;
      const req = config === undefined ? rest : { ...rest, config };
      const detail = await bridge.invoke("session_create", { req });
      const context = await contextOf(bridge, detail.id);
      await syncProjectRoot(bridge, detail.cwd);
      set((state) => ({
        busy: false,
        detail,
        events: detail.events,
        activeId: detail.id,
        openIds: opened(state.openIds, detail.id),
        context,
        sessions: [summaryOf(detail), ...state.sessions.filter((row) => row.id !== detail.id)],
      }));
      try {
        localStorage.setItem("grove.lastSession", detail.id);
      } catch {
        // Storage can be blocked; the new session is still open.
      }
      return true;
    } catch (error) {
      set({ busy: false, error: errorText(error) });
      return false;
    }
  },

  select: async (bridge, id) => {
    set({ busy: true, error: null });
    try {
      const detail = await bridge.invoke("session_get", { id });
      const context = await contextOf(bridge, id);
      // Activating a tab is choosing that project: this is the call that keeps the Specs, MCP and
      // Skills panels pointed at the session the user is actually looking at.
      await syncProjectRoot(bridge, detail.cwd);
      try {
        localStorage.setItem("grove.lastSession", id);
      } catch {
        // A blocked storage must not stop opening the conversation.
      }
      set((state) => ({
        busy: false,
        activeId: id,
        detail,
        events: detail.events,
        openIds: opened(state.openIds, id),
        context,
      }));
    } catch (error) {
      set({ busy: false, error: errorText(error) });
    }
  },

  closeTab: async (bridge, id) => {
    const state = get();
    const index = state.openIds.indexOf(id);
    const openIds = state.openIds.filter((openId) => openId !== id);
    const { [id]: _closed, ...drafts } = state.drafts;
    if (state.activeId !== id) {
      set({ openIds, drafts });
      return;
    }
    const next = openIds[Math.min(index, openIds.length - 1)] ?? null;
    set({
      openIds,
      drafts,
      activeId: null,
      detail: null,
      events: [],
      context: null,
    });
    if (next) await get().select(bridge, next);
  },

  setDraft: (id, text) => {
    set((state) => ({ drafts: { ...state.drafts, [id]: text } }));
  },

  enqueue: (id, text) => {
    const message = text.trim();
    if (!message) return;
    set((state) => ({
      queues: { ...state.queues, [id]: [...(state.queues[id] ?? []), message] },
    }));
  },

  dequeue: (id, index) => {
    set((state) => {
      const queue = [...(state.queues[id] ?? [])];
      queue.splice(index, 1);
      return { queues: { ...state.queues, [id]: queue } };
    });
  },

  flushQueue: async (bridge) => {
    const { activeId, queues, detail } = get();
    if (!activeId) return;
    const queue = queues[activeId] ?? [];
    if (queue.length === 0) return;
    if (
      detail &&
      (detail.awaiting || detail.status === "starting" || detail.status === "busy")
    ) {
      return;
    }
    const [next, ...rest] = queue;
    set((state) => ({ queues: { ...state.queues, [activeId]: rest } }));
    const accepted = await get().send(bridge, next);
    if (!accepted) {
      set((state) => ({
        queues: { ...state.queues, [activeId]: [next, ...(state.queues[activeId] ?? [])] },
      }));
    }
  },

  send: async (bridge, text) => {
    const id = get().activeId;
    const message = text.trim();
    if (!id || !message) return false;
    const detail = get().detail;
    if (detail?.status === "exited" || detail?.status === "error") {
      // The user asked to continue the conversation; starting the agent is an implementation
      // detail, not a separate action they should have to remember.
      if (!(await get().resume(bridge))) return false;
    }
    set({ error: null });
    try {
      await bridge.invoke("session_send", { id, text: message });
      return true;
    } catch (error) {
      set({ error: errorText(error) });
      return false;
    }
  },

  cancel: async (bridge) => {
    const id = get().activeId;
    if (!id) return;
    set({ error: null });
    try {
      await bridge.invoke("session_cancel", { id });
    } catch (error) {
      set({ error: errorText(error) });
    }
  },

  respondPermission: async (bridge, requestId, decision) => {
    const id = get().activeId;
    if (!id) return;
    set({ error: null });
    try {
      await bridge.invoke("session_respond_permission", { id, requestId, decision });
    } catch (error) {
      set({ error: errorText(error) });
    }
  },

  respondQuestion: async (bridge, requestId, answers) => {
    const id = get().activeId;
    if (!id) return false;
    set({ error: null });
    try {
      await bridge.invoke("session_respond_question", { id, requestId, answers });
      return true;
    } catch (error) {
      set({ error: errorText(error) });
      return false;
    }
  },

  setThinking: async (bridge, level) => {
    const id = get().activeId;
    if (!id) return false;
    set({ error: null });
    try {
      const detail = await bridge.invoke("session_set_thinking", { id, level });
      set({ detail, events: detail.events });
      return true;
    } catch (error) {
      set({ error: errorText(error) });
      return false;
    }
  },

  resume: async (bridge) => {
    const id = get().activeId;
    if (!id) return false;
    set({ busy: true, error: null });
    try {
      const session = await bridge.invoke("session_resume", { id });
      await syncProjectRoot(bridge, session.cwd);
      set({ busy: false });
      await get().select(bridge, id);
      return true;
    } catch (error) {
      set({ busy: false, error: errorText(error) });
      return false;
    }
  },

  close: async (bridge) => {
    const id = get().activeId;
    if (!id) return;
    set({ busy: true, error: null });
    try {
      await bridge.invoke("session_close", { id });
      set({ busy: false });
      await get().select(bridge, id);
    } catch (error) {
      set({ busy: false, error: errorText(error) });
    }
  },

  applyContext: (context) => {
    if (get().activeId === context.sessionId) set({ context });
  },

  refreshContext: async (bridge) => {
    const id = get().activeId;
    if (!id) return false;
    set({ busy: true, error: null });
    try {
      const context = await bridge.invoke("session_refresh_context", { id });
      set({ busy: false, context });
      return true;
    } catch (error) {
      set({ busy: false, error: errorText(error) });
      return false;
    }
  },

  switchProvider: async (bridge, provider, model) => {
    const id = get().activeId;
    if (!id) return;
    set({ busy: true, error: null });
    try {
      const detail = await bridge.invoke("session_switch_provider", { id, provider, model });
      const context = await contextOf(bridge, id);
      set((state) => ({
        busy: false,
        detail,
        events: detail.events,
        context,
        sessions: state.sessions.map((row) =>
          row.id === id
            ? { ...row, provider: detail.provider, model: detail.model, status: detail.status }
            : row,
        ),
      }));
    } catch (error) {
      set({ busy: false, error: errorText(error) });
    }
  },

  applyEvent: (sessionId, event) => {
    if (!sessionId) return;
    set((state) => {
      const isActive = state.activeId === sessionId;
      const status = event.kind === "status" ? event.state : null;
      const stopped =
        event.kind === "status" &&
        (event.state === "exited" || event.state === "error");
      const awaiting =
        event.kind === "permission_request" || event.kind === "question_request"
          ? true
          : event.kind === "permission_resolved" ||
              event.kind === "question_resolved" ||
              stopped
            ? false
            : null;
      return {
        sessions: state.sessions.map((row) =>
          row.id === sessionId
            ? {
                ...row,
                status: status ?? row.status,
                awaiting: awaiting ?? row.awaiting,
                eventCount: row.eventCount + 1,
                updatedAt: event.at,
              }
            : row,
        ),
        detail:
          isActive && state.detail
            ? {
                ...state.detail,
                status: status ?? state.detail.status,
                awaiting: awaiting ?? state.detail.awaiting,
                updatedAt: event.at,
              }
            : state.detail,
        events: isActive ? [...state.events, event] : state.events,
      };
    });
  },

  reset: () => set({ ...initial }),
}));
