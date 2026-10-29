import type {
  AddProjectRequest,
  AppInfo,
  GitBranch,
  GitWorktree,
  Project,
  SettingsPatch,
  ThinkingLevel,
  TitleModel,
} from "@grove/contract";
import { thinkingLevelSchema, titleModelSchema } from "@grove/contract";
import { create } from "zustand";
import type { GroveBridge } from "../ipc/bridge";
import { errorText } from "../ipc/errors";
import { applyTheme, DEFAULT_THEME_ID, findTheme, readBootTheme, type ThemeId, type ThemeMode } from "../workspace/theme";

export type PermissionMode = "ask" | "accept-edits" | "bypass";

export interface Preferences {
  /** Which provider a new session starts on. */
  defaultProvider: string;
  defaultModel: string;
  /** Default reasoning level for a new session. */
  thinkingLevel: ThinkingLevel;
  /** The global small model that names sessions; null means the suggested one applies. */
  titleModel: TitleModel | null;
  /** Whether an auto-titled session may be named automatically after its first reply. */
  autoTitles: boolean;
  permissionMode: PermissionMode;
  /** Absolute folder new worktrees are created under; empty means "next to the project". */
  worktreeRoot: string;
  restoreSessions: boolean;
  enabledProviders: Record<string, boolean>;
}

interface WorkspaceState {
  info: AppInfo | null;
  projects: Project[];
  worktrees: Record<string, GitWorktree[]>;
  branches: Record<string, GitBranch[]>;
  gitErrors: Record<string, string | null>;
  prefs: Preferences;
  themeId: ThemeId;
  themeMode: ThemeMode;
  loading: boolean;
  error: string | null;
  load: (bridge: GroveBridge) => Promise<void>;
  refreshProjects: (bridge: GroveBridge) => Promise<void>;
  refreshProjectGit: (bridge: GroveBridge, project: Project) => Promise<void>;
  addProject: (bridge: GroveBridge, request: AddProjectRequest) => Promise<Project>;
  /** Delete the project row; its sessions stay and come back as chats. The folder is untouched. */
  removeProject: (bridge: GroveBridge, id: string) => Promise<void>;
  setTheme: (id: ThemeId, mode: ThemeMode) => void;
  saveSettings: (bridge: GroveBridge, patch: SettingsPatch) => Promise<void>;
  setPrefs: (patch: Partial<Preferences>) => void;
  reset: () => void;
}

const DEFAULT_PREFS: Preferences = {
  defaultProvider: "anthropic",
  defaultModel: "claude-sonnet-4-5",
  thinkingLevel: "off",
  titleModel: null,
  autoTitles: true,
  permissionMode: "ask",
  worktreeRoot: "~/code",
  restoreSessions: true,
  enabledProviders: {},
};

function stringValue(value: unknown, fallback: string): string {
  return typeof value === "string" && value.trim().length > 0 ? value : fallback;
}

function booleanValue(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function thinkingLevelValue(value: unknown, fallback: ThinkingLevel): ThinkingLevel {
  const parsed = thinkingLevelSchema.safeParse(value);
  return parsed.success ? parsed.data : fallback;
}

function titleModelValue(value: unknown): TitleModel | null {
  const parsed = titleModelSchema.safeParse(value);
  return parsed.success ? parsed.data : null;
}

function prefsFromSettings(settings: Record<string, unknown>): Preferences {
  const mode = settings.permissionMode;
  return {
    defaultProvider: stringValue(settings.defaultProvider, DEFAULT_PREFS.defaultProvider),
    defaultModel: stringValue(settings.defaultModel, DEFAULT_PREFS.defaultModel),
    thinkingLevel: thinkingLevelValue(settings.thinkingLevel, DEFAULT_PREFS.thinkingLevel),
    titleModel: titleModelValue(settings.titleModel),
    autoTitles: booleanValue(settings.autoTitles, true),
    permissionMode:
      mode === "accept-edits" || mode === "bypass" || mode === "ask"
        ? mode
        : DEFAULT_PREFS.permissionMode,
    worktreeRoot: stringValue(settings.worktreeRoot, DEFAULT_PREFS.worktreeRoot),
    restoreSessions: booleanValue(settings.restoreSessions, true),
    enabledProviders:
      settings.enabledProviders && typeof settings.enabledProviders === "object"
        ? (settings.enabledProviders as Record<string, boolean>)
        : {},
  };
}

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  info: null,
  projects: [],
  worktrees: {},
  branches: {},
  gitErrors: {},
  prefs: DEFAULT_PREFS,
  themeId: readBootTheme().id,
  themeMode: readBootTheme().mode,
  loading: true,
  error: null,

  load: async (bridge) => {
    set({ loading: true, error: null });
    try {
      const [info, projects, settings] = await Promise.all([
        bridge.invoke("app_info"),
        bridge.invoke("project_list"),
        bridge.invoke("settings_get"),
      ]);
      const prefs = prefsFromSettings(settings as unknown as Record<string, unknown>);
      const theme = findTheme(
        typeof settings.themeId === "string" ? settings.themeId : get().themeId,
      );
      const mode: ThemeMode =
        settings.themeMode === "light" || settings.themeMode === "dark"
          ? settings.themeMode
          : get().themeMode;
      applyTheme(theme.id, mode);
      set({ info, projects, prefs, themeId: theme.id, themeMode: mode, loading: false });
      await Promise.all(projects.map((project) => get().refreshProjectGit(bridge, project)));
    } catch (error) {
      set({ loading: false, error: errorText(error) });
    }
  },

  refreshProjects: async (bridge) => {
    try {
      const projects = await bridge.invoke("project_list");
      set({ projects });
      await Promise.all(projects.map((project) => get().refreshProjectGit(bridge, project)));
    } catch (error) {
      set({ error: errorText(error) });
    }
  },

  refreshProjectGit: async (bridge, project) => {
    if (!project.isGit) {
      set((state) => ({
        worktrees: { ...state.worktrees, [project.id]: [] },
        branches: { ...state.branches, [project.id]: [] },
        gitErrors: { ...state.gitErrors, [project.id]: null },
      }));
      return;
    }
    try {
      const [worktrees, branches] = await Promise.all([
        bridge.invoke("git_worktrees", { repoPath: project.path }),
        bridge.invoke("git_branches", { repoPath: project.path }),
      ]);
      set((state) => ({
        worktrees: { ...state.worktrees, [project.id]: worktrees },
        branches: { ...state.branches, [project.id]: branches },
        gitErrors: { ...state.gitErrors, [project.id]: null },
      }));
    } catch (error) {
      set((state) => ({
        worktrees: { ...state.worktrees, [project.id]: [] },
        branches: { ...state.branches, [project.id]: [] },
        gitErrors: { ...state.gitErrors, [project.id]: errorText(error) },
      }));
    }
  },

  addProject: async (bridge, request) => {
    const project = await bridge.invoke("project_add", { req: request });
    set((state) => ({ projects: [...state.projects, project] }));
    await get().refreshProjectGit(bridge, project);
    return project;
  },

  removeProject: async (bridge, id) => {
    await bridge.invoke("project_remove", { id });
    set((state) => {
      const worktrees = { ...state.worktrees };
      const branches = { ...state.branches };
      const gitErrors = { ...state.gitErrors };
      delete worktrees[id];
      delete branches[id];
      delete gitErrors[id];
      return {
        projects: state.projects.filter((project) => project.id !== id),
        worktrees,
        branches,
        gitErrors,
      };
    });
  },

  setTheme: (id, mode) => {
    applyTheme(id, mode);
    set({ themeId: id, themeMode: mode });
  },

  saveSettings: async (bridge, patch) => {
    const settings = await bridge.invoke("settings_set", { patch });
    const prefs = prefsFromSettings(settings as unknown as Record<string, unknown>);
    set({ prefs });
  },

  setPrefs: (patch) => set((state) => ({ prefs: { ...state.prefs, ...patch } })),

  reset: () => {
    applyTheme(DEFAULT_THEME_ID, "dark");
    set({
      info: null,
      projects: [],
      worktrees: {},
      branches: {},
      gitErrors: {},
      prefs: DEFAULT_PREFS,
      themeId: DEFAULT_THEME_ID,
      themeMode: "dark",
      loading: true,
      error: null,
    });
  },
}));
