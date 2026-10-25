import type {
  AddProjectRequest,
  DirListing,
  Project,
  ProjectDetection,
} from "@grove/contract";

/**
 * Workspace command surface: the projects the user added and the folders the picker can browse.
 *
 * A project is durable (SQLite); its worktrees are read from git each time, because git is the
 * source of truth for a checkout.
 */
export interface ProjectsCommands {
  project_list: { args: Record<string, never>; result: Project[] };
  /** Add a folder, detecting git when it is a repository. Duplicate names/paths are rejected. */
  project_add: { args: { req: AddProjectRequest }; result: Project };
  /** What the add-project form shows before saving: git branches, default branch and remote. */
  project_detect: { args: { path: string }; result: ProjectDetection };
  /** Directories only, one level deep. `path` may be empty to start at the home folder. */
  fs_list_dirs: { args: { path?: string }; result: DirListing };
}
