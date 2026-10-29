import { create } from "zustand";

/** Shell-only state: which pane is visible, which context tab is open, and which dialog is up. */
export type ContextTab = "git" | "plan" | "spec";
export type SettingsTab = "general" | "theme" | "skills" | "providers" | "mcp";
export type PaneView = "nav" | "session" | "context";

export interface NewSessionTarget {
  /** `null` means "a chat with no project"; undefined means "whatever the app picks". */
  projectId?: string | null;
  worktreePath?: string | null;
}

export interface PullRequestSeed {
  title: string;
  body: string;
}

export interface ToastMessage {
  message: string;
  tone: "ok" | "error";
}

/** The worker whose forwarded transcript is open in place of the parent session. */
export interface SubagentSelection {
  sessionId: string;
  callId: string;
}

interface UiState {
  view: PaneView;
  setView: (view: PaneView) => void;
  contextOpen: boolean;
  setContextOpen: (open: boolean) => void;
  contextTab: ContextTab;
  setContextTab: (tab: ContextTab) => void;
  settingsTab: SettingsTab | null;
  openSettings: (tab?: SettingsTab) => void;
  closeSettings: () => void;
  newSession: NewSessionTarget | null;
  openNewSession: (target?: Partial<NewSessionTarget>) => void;
  closeNewSession: () => void;
  addProjectOpen: boolean;
  openAddProject: () => void;
  closeAddProject: () => void;
  diffPath: string | null;
  openDiff: (path: string) => void;
  closeDiff: () => void;
  prOpen: boolean;
  prSeed: PullRequestSeed | null;
  openPr: (seed?: PullRequestSeed) => void;
  closePr: () => void;
  subagent: SubagentSelection | null;
  openSubagent: (sessionId: string, callId: string) => void;
  closeSubagent: () => void;
  /** The session whose tree row is in inline-rename mode, if any; the menu starts it. */
  renamingSession: string | null;
  startRename: (id: string) => void;
  endRename: () => void;
  toast: ToastMessage | null;
  notify: (message: string, tone?: "ok" | "error") => void;
  clearToast: () => void;
  reset: () => void;
}

export const useUiStore = create<UiState>((set) => ({
  view: "session",
  setView: (view) => set({ view }),
  contextOpen: true,
  setContextOpen: (contextOpen) => set({ contextOpen }),
  contextTab: "git",
  setContextTab: (contextTab) => set({ contextTab }),
  settingsTab: null,
  openSettings: (settingsTab = "general") => set({ settingsTab }),
  closeSettings: () => set({ settingsTab: null }),
  newSession: null,
  openNewSession: (target) =>
    set({
      newSession: {
        projectId: target?.projectId,
        worktreePath: target?.worktreePath,
      },
    }),
  closeNewSession: () => set({ newSession: null }),
  addProjectOpen: false,
  openAddProject: () => set({ addProjectOpen: true }),
  closeAddProject: () => set({ addProjectOpen: false }),
  diffPath: null,
  openDiff: (diffPath) => set({ diffPath }),
  closeDiff: () => set({ diffPath: null }),
  prOpen: false,
  prSeed: null,
  openPr: (prSeed) => set({ prOpen: true, prSeed: prSeed ?? null }),
  closePr: () => set({ prOpen: false, prSeed: null }),
  subagent: null,
  openSubagent: (sessionId, callId) => set({ subagent: { sessionId, callId } }),
  closeSubagent: () => set({ subagent: null }),
  renamingSession: null,
  startRename: (id) => set({ renamingSession: id }),
  endRename: () => set({ renamingSession: null }),
  toast: null,
  notify: (message, tone = "ok") => set({ toast: { message, tone } }),
  clearToast: () => set({ toast: null }),
  reset: () =>
    set({
      view: "session",
      contextOpen: true,
      contextTab: "git",
      settingsTab: null,
      newSession: null,
      addProjectOpen: false,
      diffPath: null,
      prOpen: false,
      prSeed: null,
      subagent: null,
      renamingSession: null,
      toast: null,
    }),
}));
