import type {
  CatalogEntry,
  InstallRecord,
  SkillCatalog,
  SkillPreview,
  SkillTarget,
} from "@grove/contract";
import { create } from "zustand";
import type { GroveBridge } from "../ipc/bridge";
import { errorText } from "../ipc/errors";

interface SkillsState {
  catalog: SkillCatalog | null;
  installs: InstallRecord[];
  targets: SkillTarget[];
  preview: SkillPreview | null;
  selectedId: string | null;
  query: string;
  loading: boolean;
  previewLoading: boolean;
  installing: string | null;
  error: string | null;
  load: (bridge: GroveBridge, projectRoot?: string, refresh?: boolean) => Promise<void>;
  loadInstalls: (bridge: GroveBridge) => Promise<void>;
  select: (bridge: GroveBridge, entry: CatalogEntry) => void;
  setQuery: (query: string) => void;
  install: (bridge: GroveBridge, entry: CatalogEntry, projectRoot?: string) => Promise<boolean>;
  remove: (bridge: GroveBridge, record: InstallRecord) => Promise<boolean>;
}

export const useSkillsStore = create<SkillsState>((set, get) => ({
  catalog: null,
  installs: [],
  targets: [],
  preview: null,
  selectedId: null,
  query: "",
  loading: false,
  previewLoading: false,
  installing: null,
  error: null,

  load: async (bridge, projectRoot, refresh = false) => {
    set({ loading: true, error: null });
    try {
      const [catalog, installs, targets] = await Promise.all([
        bridge.invoke("skill_catalog", { refresh }),
        bridge.invoke("skill_installs"),
        bridge.invoke("skill_targets", projectRoot ? { projectRoot } : {}),
      ]);
      set({ catalog, installs, targets, loading: false });
      const selected = get().selectedId;
      if (!selected && catalog.entries.length > 0) {
        get().select(bridge, catalog.entries[0]);
      }
    } catch (error) {
      set({ loading: false, error: errorText(error) });
    }
  },

  loadInstalls: async (bridge) => {
    try {
      const installs = await bridge.invoke("skill_installs");
      set({ installs });
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

  install: async (bridge, entry, projectRoot) => {
    set({ installing: entry.id, error: null });
    try {
      await bridge.invoke("skill_install", {
        source: entry.source,
        path: entry.path,
        target: "agents",
        ...(projectRoot ? { projectRoot } : {}),
      });
      const [installs, targets] = await Promise.all([
        bridge.invoke("skill_installs"),
        bridge.invoke("skill_targets", projectRoot ? { projectRoot } : {}),
      ]);
      set({ installs, targets, installing: null });
      return true;
    } catch (error) {
      set({ installing: null, error: errorText(error) });
      return false;
    }
  },

  remove: async (bridge, record) => {
    set({ error: null });
    try {
      await bridge.invoke("skill_remove", { path: record.path });
      const installs = await bridge.invoke("skill_installs");
      set({ installs });
      return true;
    } catch (error) {
      set({ error: errorText(error) });
      return false;
    }
  },
}));
