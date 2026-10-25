import type {
  InstallRecord,
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
  /** Every recorded installation, newest last. */
  skill_installs: { args: Record<string, never>; result: InstallRecord[] };
  /** Delete one installed skill folder by the recorded path. */
  skill_remove: { args: { path: string }; result: null };
}
