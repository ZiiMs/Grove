import type {
  InstallResult,
  SkillCatalog,
  SkillPreview,
  SkillTarget,
} from "@grove/contract";

/** Skill catalog commands. One interface per domain; `index.ts` joins them. */
export interface SkillCommands {
  skill_catalog: { args: { refresh?: boolean }; result: SkillCatalog };
  skill_preview: { args: { source: string; path: string }; result: SkillPreview };
  skill_targets: { args: { projectRoot?: string }; result: SkillTarget[] };
  skill_install: {
    args: { source: string; path: string; target: string; projectRoot?: string };
    result: InstallResult;
  };
}
