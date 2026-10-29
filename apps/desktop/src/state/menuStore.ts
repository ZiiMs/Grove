import { create } from "zustand";
import type { IconName } from "../workspace/Icon";

/**
 * One app-level context menu. Components never render their own popover: they put a [`MenuState`] in
 * this store, and the single host in the shell draws it in the topmost layer.
 */

export type MenuItem =
  | {
      kind: "item";
      label: string;
      icon?: IconName;
      /** Right-aligned hint, e.g. `Ctrl+Z`. */
      shortcut?: string;
      danger?: boolean;
      disabled?: boolean;
      onSelect: () => void;
    }
  | { kind: "separator" };

/** Where a surface opened its menu: the pointer or trigger, and where focus goes when it closes. */
export interface MenuAnchor {
  x: number;
  y: number;
  returnFocus: HTMLElement | null;
}

export interface MenuState extends MenuAnchor {
  items: MenuItem[];
}

interface MenuStore {
  menu: MenuState | null;
  openMenu: (menu: MenuState) => void;
  closeMenu: () => void;
  /** Test and teardown hook; production closes through `closeMenu`. */
  reset: () => void;
}

export const useMenuStore = create<MenuStore>((set) => ({
  menu: null,
  openMenu: (menu) => set({ menu }),
  closeMenu: () => set({ menu: null }),
  reset: () => set({ menu: null }),
}));
