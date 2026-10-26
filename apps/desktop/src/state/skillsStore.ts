import type {
  CatalogEntry,
  DetectedSkill,
  InstallRecord,
  SkillCatalog,
  SkillPreview,
  SkillTarget,
} from "@grove/contract";
import { create } from "zustand";
import type { GroveBridge } from "../ipc/bridge";
import { errorText } from "../ipc/errors";

function normalizeSource(input: string): string {
  return input
    .trim()
    .replace(/^https?:\/\/(?:www\.)?github\.com\//i, "")
    .split(/[?#]/)[0]
    .replace(/\/+$/, "")
    .replace(/\.git$/i, "");
}

interface SkillsState {
  catalog: SkillCatalog | null;
  installs: InstallRecord[];
  /** Skill folders found on disk under either target, including ones Grove did not install. */
  inventory: DetectedSkill[];
  /** Configured GitHub catalogs, even those with no skills yet. */
  sources: string[];
  targets: SkillTarget[];
  preview: SkillPreview | null;
  selectedId: string | null;
  query: string;
  loading: boolean;
  previewLoading: boolean;
  installing: string | null;
  refreshing: boolean;
  error: string | null;
  load: (bridge: GroveBridge, projectRoot?: string, refresh?: boolean) => Promise<void>;
  loadInstalls: (bridge: GroveBridge, projectRoot?: string) => Promise<void>;
  select: (bridge: GroveBridge, entry: CatalogEntry) => void;
  setQuery: (query: string) => void;
  install: (
    bridge: GroveBridge,
    entries: CatalogEntry[],
    target: string,
    projectRoot?: string,
  ) => Promise<{ installed: number; error: string | null }>;
  remove: (bridge: GroveBridge, path: string, projectRoot?: string) => Promise<boolean>;
  addSource: (
    bridge: GroveBridge,
    input: string,
  ) => Promise<{ ok: boolean; error: string | null; source: string }>;
  removeAll: (
    bridge: GroveBridge,
    paths: string[],
    projectRoot?: string,
  ) => Promise<{ removed: number; error: string | null }>;
}

export const useSkillsStore = create<SkillsState>((set, get) => ({
  catalog: null,
  installs: [],
  inventory: [],
  sources: [],
  targets: [],
  preview: null,
  selectedId: null,
  query: "",
  loading: false,
  previewLoading: false,
  installing: null,
  refreshing: false,
  error: null,

  load: async (bridge, projectRoot, refresh = false) => {
    set({
      loading: get().catalog === null,
      refreshing: get().catalog !== null,
      error: null,
    });
    try {
      const [catalog, installs, inventory, targets, settings] = await Promise.all([
        bridge.invoke("skill_catalog", { refresh }),
        bridge.invoke("skill_installs"),
        bridge.invoke("skill_inventory", projectRoot ? { projectRoot } : {}),
        bridge.invoke("skill_targets", projectRoot ? { projectRoot } : {}),
        bridge.invoke("settings_get").catch(() => null),
      ]);
      const configured = settings?.catalogSources ?? [];
      const sources = [
        ...new Set([...configured, ...catalog.entries.map((entry) => entry.source)]),
      ];
      set({
        catalog,
        installs,
        inventory,
        sources,
        targets,
        loading: false,
        refreshing: false,
      });
      const selected = get().selectedId;
      if (!selected && catalog.entries.length > 0) {
        get().select(bridge, catalog.entries[0]);
      }
    } catch (error) {
      set({ loading: false, refreshing: false, error: errorText(error) });
    }
  },

  loadInstalls: async (bridge, projectRoot) => {
    try {
      const [installs, inventory] = await Promise.all([
        bridge.invoke("skill_installs"),
        bridge.invoke("skill_inventory", projectRoot ? { projectRoot } : {}),
      ]);
      set({ installs, inventory });
    } catch {
      // The badge is cosmetic; a failed read leaves the previous count.
    }
  },

  select: (bridge, entry) => {
    set({ selectedId: entry.id });
    void (async () => {
      set({ previewLoading: true });
      try {
        const preview = await bridge.invoke("skill_preview", {
          source: entry.source,
          path: entry.path,
        });
        if (get().selectedId === entry.id) set({ preview, previewLoading: false, error: null });
      } catch (error) {
        if (get().selectedId === entry.id) {
          set({ preview: null, previewLoading: false, error: errorText(error) });
        }
      }
    })();
  },

  setQuery: (query) => set({ query }),

  install: async (bridge, entries, target, projectRoot) => {
    if (entries.length === 0) return { installed: 0, error: null };
    set({
      installing: entries.length > 1 ? "*" : entries[0].id,
      error: null,
    });
    let installed = 0;
    let error: string | null = null;
    for (const entry of entries) {
      try {
        await bridge.invoke("skill_install", {
          source: entry.source,
          path: entry.path,
          target,
          ...(projectRoot ? { projectRoot } : {}),
        });
        installed += 1;
      } catch (err) {
        error = errorText(err);
        break;
      }
    }
    try {
      const [installs, inventory, targets] = await Promise.all([
        bridge.invoke("skill_installs"),
        bridge.invoke("skill_inventory", projectRoot ? { projectRoot } : {}),
        bridge.invoke("skill_targets", projectRoot ? { projectRoot } : {}),
      ]);
      set({ installs, inventory, targets, installing: null, error });
    } catch (err) {
      set({ installing: null, error: error ?? errorText(err) });
    }
    return { installed, error };
  },

  remove: async (bridge, path, projectRoot) => {
    set({ error: null });
    try {
      await bridge.invoke("skill_remove", {
        path,
        ...(projectRoot ? { projectRoot } : {}),
      });
      const [installs, inventory] = await Promise.all([
        bridge.invoke("skill_installs"),
        bridge.invoke("skill_inventory", projectRoot ? { projectRoot } : {}),
      ]);
      set({ installs, inventory });
      return true;
    } catch (error) {
      set({ error: errorText(error) });
      return false;
    }
  },

  addSource: async (bridge, input) => {
    const source = normalizeSource(input);
    if (!source) {
      return { ok: false, error: "Enter an owner/repo GitHub repository.", source };
    }
    try {
      const settings = await bridge.invoke("settings_get");
      const existing = settings.catalogSources ?? [];
      if (existing.some((item) => item.toLowerCase() === source.toLowerCase())) {
        return { ok: false, error: "That catalog is already added.", source };
      }
      const saved = await bridge.invoke("settings_set", {
        patch: { catalogSources: [...existing, source] },
      });
      set({ sources: saved.catalogSources });
      try {
        const catalog = await bridge.invoke("skill_catalog", { refresh: true });
        set({ catalog });
      } catch (error) {
        return { ok: true, error: errorText(error), source };
      }
      return { ok: true, error: null, source };
    } catch (error) {
      return { ok: false, error: errorText(error), source };
    }
  },

  removeAll: async (bridge, paths, projectRoot) => {
    if (paths.length === 0) return { removed: 0, error: null };
    set({ error: null });
    let removed = 0;
    let error: string | null = null;
    for (const path of paths) {
      try {
        await bridge.invoke("skill_remove", {
          path,
          ...(projectRoot ? { projectRoot } : {}),
        });
        removed += 1;
      } catch (err) {
        error = errorText(err);
      }
    }
    try {
      const [installs, inventory] = await Promise.all([
        bridge.invoke("skill_installs"),
        bridge.invoke("skill_inventory", projectRoot ? { projectRoot } : {}),
      ]);
      set({ installs, inventory });
    } catch (err) {
      set({ error: error ?? errorText(err) });
    }
    return { removed, error };
  },
}));
