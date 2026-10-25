import type { ComponentType } from "react";
import { GitPanel } from "./git";
import { McpPanel } from "./mcp";
import { PlanPanel } from "./plan";
import { SessionsPanel } from "./sessions";
import { SettingsPanel } from "./settings";
import { SkillsPanel } from "./skills";
import { SpecsPanel } from "./specs";

export type PanelId = "sessions" | "skills" | "specs" | "plan" | "mcp" | "git" | "settings";

export interface PanelDefinition {
  id: PanelId;
  title: string;
  glyph: string;
  Component: ComponentType;
}

/**
 * The panel seam: one entry per panel, one line per addition. Each panel owns its directory; tickets
 * replace the component in place, never this list.
 */
export const panels: PanelDefinition[] = [
  { id: "sessions", title: "Sessions", glyph: "▶", Component: SessionsPanel },
  { id: "skills", title: "Skills", glyph: "✦", Component: SkillsPanel },
  { id: "specs", title: "Specs", glyph: "◈", Component: SpecsPanel },
  { id: "plan", title: "Plan", glyph: "☰", Component: PlanPanel },
  { id: "mcp", title: "MCP", glyph: "⚙", Component: McpPanel },
  { id: "git", title: "Git", glyph: "⑂", Component: GitPanel },
  { id: "settings", title: "Settings", glyph: "⚑", Component: SettingsPanel },
];
