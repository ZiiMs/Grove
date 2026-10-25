/**
 * PROTOTYPE — throwaway. Section registry. `surface` says where a section renders in the real shell:
 *   chat  — the centre (chat is always there)
 *   side  — one of the right panel's four tabs
 *   modal — a section of the near-fullscreen Settings modal
 */

import type { ComponentType } from "react";
import { ChatSection } from "./Chat";
import { ControlsSection } from "./Controls";
import { DataSection } from "./Data";
import { ShellSection } from "./Shell";
import { SidepanelSection } from "./Sidepanel";
import type { VariantDef } from "../panels/types";
import { ChangesView } from "../panels/Changes";
import { gitVariants } from "../panels/Git";
import { mcpVariants } from "../panels/Mcp";
import { planVariants } from "../panels/Plan";
import { settingsVariants } from "../panels/Settings";
import { sessionsVariants } from "../panels/Sessions";
import { skillsVariants } from "../panels/Skills";
import { specsVariants } from "../panels/Specs";

export type Surface = "chat" | "side" | "modal";

export type SectionId =
  | "theme"
  | "shell"
  | "sidepanel"
  | "controls"
  | "chat"
  | "data"
  | "sessions"
  | "plan"
  | "specs"
  | "changes"
  | "git"
  | "skills"
  | "mcp"
  | "settings";

export interface SectionDef {
  id: SectionId;
  title: string;
  glyph: string;
  group: "guide" | "panels";
  surface: Surface;
  blurb?: string;
  variants?: VariantDef[];
}

export const sections: SectionDef[] = [
  { id: "theme", title: "Theme", glyph: "◐", group: "guide", surface: "chat", blurb: "Presets and knobs" },
  { id: "shell", title: "Shell & chrome", glyph: "▭", group: "guide", surface: "chat", blurb: "Nav, chat header, split panes" },
  { id: "sidepanel", title: "Sidepanel", glyph: "▥", group: "guide", surface: "chat", blurb: "Right column: tabs, contents, collapse" },
  { id: "controls", title: "Controls", glyph: "⏺", group: "guide", surface: "chat", blurb: "Buttons, inputs, type" },
  { id: "chat", title: "Chat & agent", glyph: "▶", group: "guide", surface: "chat", blurb: "Turns, tools, diffs" },
  { id: "data", title: "Data & status", glyph: "◈", group: "guide", surface: "chat", blurb: "Trees, tables, states" },
  { id: "sessions", title: "Chat", glyph: "▶", group: "panels", surface: "chat", variants: sessionsVariants },
  { id: "plan", title: "Plan", glyph: "☰", group: "panels", surface: "side", variants: planVariants },
  { id: "specs", title: "Specs", glyph: "◈", group: "panels", surface: "side", variants: specsVariants },
  { id: "changes", title: "Changes", glyph: "±", group: "panels", surface: "side" },
  { id: "git", title: "Git", glyph: "⑂", group: "panels", surface: "side", variants: gitVariants },
  { id: "skills", title: "Skill catalog", glyph: "✦", group: "panels", surface: "modal", variants: skillsVariants },
  { id: "mcp", title: "MCP", glyph: "⚙", group: "panels", surface: "modal", variants: mcpVariants },
  { id: "settings", title: "Settings", glyph: "⚑", group: "panels", surface: "modal", variants: settingsVariants },
];

export const guideComponents: Partial<Record<SectionId, ComponentType>> = {
  shell: ShellSection,
  sidepanel: SidepanelSection,
  controls: ControlsSection,
  chat: ChatSection,
  data: DataSection,
};

export function findSection(id: string | null): SectionDef {
  return sections.find((section) => section.id === id) ?? sections[0];
}

/** No key (or an unknown one) resolves to the section's chosen layout, else its first. */
export function findVariant(section: SectionDef, key: string | null): VariantDef | null {
  if (!section.variants || section.variants.length === 0) return null;
  const match = key ? section.variants.find((variant) => variant.key === key) : undefined;
  return match ?? section.variants.find((variant) => variant.chosen) ?? section.variants[0];
}

export function defaultVariantKey(section: SectionDef): string {
  return findVariant(section, null)?.key ?? "A";
}

/** The right panel's four tabs, in order. */
export const sideSections = sections.filter((section) => section.surface === "side");

/** Everything reachable from the Settings modal. */
export const modalSections = sections.filter((section) => section.surface === "modal");

/** The centre is chat unless a guide section is being inspected. */
export const chatSection = findSection("sessions");

export const guideSections = sections.filter((section) => section.group === "guide");

/** Changes has no variants, so it needs its component named explicitly. */
export const changesComponent: ComponentType = ChangesView;
