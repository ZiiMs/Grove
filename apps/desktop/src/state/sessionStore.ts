import type {
  Decision,
  HarnessEvent,
  HarnessInfo,
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
  harness: string;
  cwd?: string | null;
  model?: string | null;
  title?: string | null;
  /**
   * Adapter-specific config (`config.command`, `config.args`). No form field sets it yet, but the
   * command carries it: it is how a caller — a test, or a future "ACP command" field — launches a
   * harness other than its default binary. Omitted when undefined, so the request stays minimal.
   */
  config?: unknown;
}

interface SessionStoreData {
  harnesses: HarnessInfo[];
  sessions: SessionSummary[];
  activeId: string | null;
  detail: SessionDetail | null;
  events: HarnessEvent[];
  /** Session ids open as tabs, in the order they were opened. */
  openIds: string[];
  /** Compose text per session: a draft belongs to its tab, not to the panel. */
  drafts: Record<string, string>;
  /** The active session's knowledge (pins, plan, the documents a start would send). */
  context: SessionContext | null;
  loading: boolean;
  busy: boolean;
  error: string | null;
}

export interface SessionStore extends SessionStoreData {
  /** Harness picker contents and the session list. */
  load: (bridge: GroveBridge) => Promise<void>;
  create: (bridge: GroveBridge, input: CreateSessionInput) => Promise<void>;
  /** Open (or focus) a session as a tab. */
  select: (bridge: GroveBridge, id: string) => Promise<void>;
  /** Close a tab; the next open one becomes active. The session itself is untouched. */
  closeTab: (bridge: GroveBridge, id: string) => Promise<void>;
  setDraft: (id: string, text: string) => void;
  /** Returns whether the message was accepted, so the panel knows when to clear the compose box. */
  send: (bridge: GroveBridge, text: string) => Promise<boolean>;
  cancel: (bridge: GroveBridge) => Promise<void>;
  /**
   * Answer a pending permission request for the active session. The transcript does not change here:
   * the buttons disappear when the harness's `permission_resolved` event arrives, so the UI shows
   * what the harness actually accepted.
   */
  respondPermission: (bridge: GroveBridge, requestId: string, decision: Decision) => Promise<void>;
  resume: (bridge: GroveBridge) => Promise<void>;
  close: (bridge: GroveBridge) => Promise<void>;
  /** Adopt a snapshot another panel produced (the Specs panel pins and unpins). */
  applyContext: (context: SessionContext) => void;
  /** Re-send the current pins and plan to the running harness, then show the new snapshot. */
  refreshContext: (bridge: GroveBridge) => Promise<void>;
  /** Re-launch the active session on another registered harness, keeping its transcript. */
  switchHarness: (bridge: GroveBridge, harness: string) => Promise<void>;
  /** Fold one live `grove://session-event` into the list and, when it belongs to the open session, the transcript. */
  applyEvent: (sessionId: string | null, event: HarnessEvent) => void;
  reset: () => void;
}

const initial: SessionStoreData = {
  harnesses: [],
  sessions: [],
  activeId: null,
  detail: null,
  events: [],
  openIds: [],
  drafts: {},
  context: null,
  loading: true,
  busy: false,
  error: null,
};


function summaryOf(detail: SessionDetail): SessionSummary {
  const { events, ...session } = detail;
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
      const [harnesses, sessions] = await Promise.all([
        bridge.invoke("harness_list"),
        bridge.invoke("session_list"),
      ]);
      set({ harnesses, sessions, loading: false });
    } catch (error) {
      set({ loading: false, error: errorText(error) });
    }
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
    } catch (error) {
      set({ busy: false, error: errorText(error) });
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

  send: async (bridge, text) => {
    const id = get().activeId;
    const message = text.trim();
    if (!id || !message) return false;
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

  resume: async (bridge) => {
    const id = get().activeId;
    if (!id) return;
    set({ busy: true, error: null });
    try {
      const session = await bridge.invoke("session_resume", { id });
      await syncProjectRoot(bridge, session.cwd);
      set({ busy: false });
      await get().select(bridge, id);
    } catch (error) {
      set({ busy: false, error: errorText(error) });
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
    if (!id) return;
    set({ busy: true, error: null });
    try {
      const context = await bridge.invoke("session_refresh_context", { id });
      set({ busy: false, context });
    } catch (error) {
      set({ busy: false, error: errorText(error) });
    }
  },

  switchHarness: async (bridge, harness) => {
    const id = get().activeId;
    if (!id) return;
    set({ busy: true, error: null });
    try {
      const detail = await bridge.invoke("session_switch_harness", { id, harness });
      const context = await contextOf(bridge, id);
      set((state) => ({
        busy: false,
        detail,
        events: detail.events,
        context,
        sessions: state.sessions.map((row) =>
          row.id === id ? { ...row, harness: detail.harness, status: detail.status } : row,
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
      return {
        sessions: state.sessions.map((row) =>
          row.id === sessionId
            ? {
                ...row,
                status: status ?? row.status,
                eventCount: row.eventCount + 1,
                updatedAt: event.at,
              }
            : row,
        ),
        detail:
          isActive && state.detail
            ? { ...state.detail, status: status ?? state.detail.status, updatedAt: event.at }
            : state.detail,
        events: isActive ? [...state.events, event] : state.events,
      };
    });
  },

  reset: () => set({ ...initial }),
}));
