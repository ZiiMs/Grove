import type {
  GitDiff,
  GitStatus,
  Plan,
  PrResult,
  Project,
  Session,
  SpecDoc,
  SpecGraph,
} from "@grove/contract";
import { create } from "zustand";
import type { GroveBridge } from "../ipc/bridge";
import { errorText } from "../ipc/errors";
import { statsFromPatch, totalStats, type FileStat } from "../workspace/gitStats";

/**
 * The context panel's data: the active checkout's git status, the session's plan and the project's
 * spec graph. Kept out of the session store because all three are re-read when the active
 * project/worktree moves, while the transcript stays put.
 */
interface ContextState {
  activeSessionId: string | null;
  activePath: string | null;
  git: GitStatus | null;
  gitStats: FileStat | null;
  fileStats: Record<string, FileStat>;
  gitError: string | null;
  gitLoading: boolean;
  plan: Plan | null;
  planError: string | null;
  specs: SpecGraph | null;
  specError: string | null;
  specDoc: SpecDoc | null;
  specDocPath: string | null;
  refreshFor: (bridge: GroveBridge, session: Session | null, project: Project | null) => Promise<void>;
  refreshGit: (bridge: GroveBridge, path: string | null) => Promise<void>;
  refreshPlan: (bridge: GroveBridge, sessionId: string) => Promise<void>;
  refreshSpecs: (bridge: GroveBridge) => Promise<void>;
  loadSpecDoc: (bridge: GroveBridge, path: string) => Promise<void>;
  closeSpecDoc: () => void;
  applyPlan: (sessionId: string | null, plan: Plan) => void;
  stage: (bridge: GroveBridge, paths: string[], staged: boolean) => Promise<GitStatus | null>;
  commit: (bridge: GroveBridge, message: string, paths: string[]) => Promise<GitStatus | null>;
  openPr: (bridge: GroveBridge, args: {
    title: string;
    body: string;
    base?: string;
    head?: string;
    draft: boolean;
  }) => Promise<PrResult | null>;
  diff: (bridge: GroveBridge, path: string) => Promise<GitDiff | null>;
  reset: () => void;
}

/**
 * Per-file counts for the whole checkout. `git diff` omits untracked files, so those are asked for
 * one at a time; the result is what the titlebar chip and the changes list read.
 */
async function computeStats(
  bridge: GroveBridge,
  path: string,
  files: GitStatus["files"],
): Promise<{ fileStats: Record<string, FileStat>; gitStats: FileStat | null }> {
  if (files.length === 0) return { fileStats: {}, gitStats: null };
  try {
    const diff = await bridge.invoke("git_diff", { repoPath: path });
    const parsed = statsFromPatch(diff.patch);
    const fileStats: Record<string, FileStat> = {};
    for (const file of files) fileStats[file.path] = parsed[file.path] ?? { added: 0, removed: 0 };
    const untracked = files.filter((file) => file.untracked);
    const extra = await Promise.all(
      untracked.map(async (file) => {
        try {
          const fileDiff = await bridge.invoke("git_diff", { repoPath: path, path: file.path });
          return [file.path, statsFromPatch(fileDiff.patch)[file.path] ?? { added: 0, removed: 0 }] as const;
        } catch {
          return [file.path, { added: 0, removed: 0 }] as const;
        }
      }),
    );
    for (const [filePath, stat] of extra) fileStats[filePath] = stat;
    return { fileStats, gitStats: totalStats(fileStats) };
  } catch {
    const fileStats: Record<string, FileStat> = {};
    for (const file of files) fileStats[file.path] = { added: 0, removed: 0 };
    return { fileStats, gitStats: { added: 0, removed: 0 } };
  }
}

export const useContextStore = create<ContextState>((set, get) => ({
  activeSessionId: null,
  activePath: null,
  git: null,
  gitStats: null,
  fileStats: {},
  gitError: null,
  gitLoading: false,
  plan: null,
  planError: null,
  specs: null,
  specError: null,
  specDoc: null,
  specDocPath: null,

  refreshFor: async (bridge, session, _project) => {
    set({
      activeSessionId: session?.id ?? null,
      activePath: session?.cwd ?? null,
      specDoc: null,
      specDocPath: null,
      gitStats: null,
      fileStats: {},
    });
    if (!session) {
      set({ git: null, gitStats: null, fileStats: {}, plan: null, gitError: null, planError: null });
      await get().refreshSpecs(bridge);
      return;
    }
    await Promise.all([
      get().refreshGit(bridge, session.cwd),
      get().refreshPlan(bridge, session.id),
      get().refreshSpecs(bridge),
    ]);
  },

  refreshGit: async (bridge, path) => {
    if (!path) {
      set({ git: null, gitStats: null, fileStats: {}, gitError: null, gitLoading: false });
      return;
    }
    set({ gitLoading: true });
    try {
      const git = await bridge.invoke("git_status", { repoPath: path });
      const stats = await computeStats(bridge, path, git.files);
      if (get().activePath === path) {
        set({ git, ...stats, gitError: null, gitLoading: false });
      }
    } catch (error) {
      if (get().activePath === path) {
        set({ git: null, gitStats: null, fileStats: {}, gitError: errorText(error), gitLoading: false });
      }
    }
  },

  refreshPlan: async (bridge, sessionId) => {
    try {
      const plan = await bridge.invoke("plan_get", { id: sessionId });
      if (get().activeSessionId === sessionId) set({ plan, planError: null });
    } catch (error) {
      if (get().activeSessionId === sessionId) {
        set({ plan: null, planError: errorText(error) });
      }
    }
  },

  refreshSpecs: async (bridge) => {
    try {
      const specs = await bridge.invoke("spec_index");
      set({ specs, specError: null });
    } catch (error) {
      set({ specs: null, specError: errorText(error) });
    }
  },

  loadSpecDoc: async (bridge, path) => {
    try {
      const specDoc = await bridge.invoke("spec_read", { path });
      set({ specDoc, specDocPath: path, specError: null });
    } catch (error) {
      set({ specError: errorText(error) });
    }
  },

  closeSpecDoc: () => set({ specDoc: null, specDocPath: null }),

  applyPlan: (sessionId, plan) => {
    if (sessionId === null || sessionId === get().activeSessionId) set({ plan });
  },

  stage: async (bridge, paths, staged) => {
    const path = get().activePath;
    if (!path || paths.length === 0) return null;
    try {
      const git = await bridge.invoke("git_stage", { repoPath: path, paths, staged });
      const stats = await computeStats(bridge, path, git.files);
      if (get().activePath === path) set({ git, ...stats, gitError: null });
      return git;
    } catch (error) {
      set({ gitError: errorText(error) });
      return null;
    }
  },

  commit: async (bridge, message, paths) => {
    const path = get().activePath;
    if (!path) return null;
    try {
      const git = await bridge.invoke("git_commit", { repoPath: path, message, paths });
      const stats = await computeStats(bridge, path, git.files);
      if (get().activePath === path) set({ git, ...stats, gitError: null });
      return git;
    } catch (error) {
      set({ gitError: errorText(error) });
      throw error;
    }
  },

  openPr: async (bridge, args) => {
    const path = get().activePath;
    if (!path) return null;
    try {
      return await bridge.invoke("git_open_pr", { repoPath: path, ...args });
    } catch (error) {
      set({ gitError: errorText(error) });
      throw error;
    }
  },

  diff: async (bridge, path) => {
    const repoPath = get().activePath;
    if (!repoPath) return null;
    try {
      return await bridge.invoke("git_diff", { repoPath, path });
    } catch (error) {
      set({ gitError: errorText(error) });
      return null;
    }
  },

  reset: () =>
    set({
      activeSessionId: null,
      activePath: null,
      git: null,
      gitStats: null,
      fileStats: {},
      gitError: null,
      gitLoading: false,
      plan: null,
      planError: null,
      specs: null,
      specError: null,
      specDoc: null,
      specDocPath: null,
    }),
}));
