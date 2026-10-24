import { create } from "zustand";
import type { PanelId } from "../panels/registry";

interface UiState {
  activePanel: PanelId;
  setActivePanel: (panel: PanelId) => void;
}

/** Shell state only: which panel is mounted. Domain state lives in its own store. */
export const useUiStore = create<UiState>((set) => ({
  activePanel: "sessions",
  setActivePanel: (activePanel) => set({ activePanel }),
}));
