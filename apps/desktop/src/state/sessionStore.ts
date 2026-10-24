import type { HarnessEvent, HarnessInfo, SessionDetail, SessionSummary } from "@grove/contract";
import { create } from "zustand";
import type { GroveBridge } from "../ipc/bridge";

/**
 * Session domain state: the list, the open transcript, and the commands that move it. Panels render
 * from here and pass the bridge in, so the store never imports Tauri and every UI test runs under
 * `FakeBridge`.
 */

export interface CreateSessionInput {
  harness: string;
  cwd?: string | null;
  model?: string | null;
  title?: string | null;
}

interface SessionStoreData {
  harnesses: HarnessInfo[];
  sessions: SessionSummary[];
  activeId: string | null;
  detail: SessionDetail | null;
  events: HarnessEvent[];
  loading: boolean;
  busy: boolean;
  error: string | null;
}

export interface SessionStore extends SessionStoreData {
  /** Harness picker contents and the session list. */
  load: (bridge: GroveBridge) => Promise<void>;
  create: (bridge: GroveBridge, input: CreateSessionInput) => Promise<void>;
  select: (bridge: GroveBridge, id: string) => Promise<void>;
  /** Returns whether the message was accepted, so the panel knows when to clear the compose box. */
  send: (bridge: GroveBridge, text: string) => Promise<boolean>;
  cancel: (bridge: GroveBridge) => Promise<void>;
  resume: (bridge: GroveBridge) => Promise<void>;
  close: (bridge: GroveBridge) => Promise<void>;
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
  loading: true,
  busy: false,
  error: null,
};

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

function summaryOf(detail: SessionDetail): SessionSummary {
  const { events, ...session } = detail;
  return { ...session, eventCount: events.length };
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
      set({ loading: false, error: errorMessage(error) });
    }
  },

  create: async (bridge, input) => {
    set({ busy: true, error: null });
    try {
      const detail = await bridge.invoke("session_create", { req: input });
      set((state) => ({
        busy: false,
        detail,
        events: detail.events,
        activeId: detail.id,
        sessions: [summaryOf(detail), ...state.sessions.filter((row) => row.id !== detail.id)],
      }));
    } catch (error) {
      set({ busy: false, error: errorMessage(error) });
    }
  },

  select: async (bridge, id) => {
    set({ busy: true, error: null });
    try {
      const detail = await bridge.invoke("session_get", { id });
      set({ busy: false, activeId: id, detail, events: detail.events });
    } catch (error) {
      set({ busy: false, error: errorMessage(error) });
    }
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
      set({ error: errorMessage(error) });
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
      set({ error: errorMessage(error) });
    }
  },

  resume: async (bridge) => {
    const id = get().activeId;
    if (!id) return;
    set({ busy: true, error: null });
    try {
      await bridge.invoke("session_resume", { id });
      set({ busy: false });
      await get().select(bridge, id);
    } catch (error) {
      set({ busy: false, error: errorMessage(error) });
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
      set({ busy: false, error: errorMessage(error) });
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
