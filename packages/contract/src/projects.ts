import { z } from "zod";

/**
 * Workspace contract: the projects the user added, the folders the add-project picker can browse,
 * and what git detection reports before a project is saved.
 *
 * Rust (`grove_core::projects`) owns the wire shape; these schemas must accept exactly what it
 * serialises. Both sides are pinned by `contract/fixtures/project*.json`,
 * `contract/fixtures/dir-listing.json`, and `contract/fixtures/fs-*.json`.
 */

export const projectSchema = z.object({
  id: z.string(),
  name: z.string(),
  /** Forward-slash absolute path, as the user picked it. */
  path: z.string(),
  /** `owner/name`, or null when the folder has no git remote. */
  remote: z.string().nullable(),
  /** Branch new sessions branch from / PRs target; null for a non-git folder. */
  defaultBranch: z.string().nullable(),
  isGit: z.boolean(),
  createdAt: z.string(),
  updatedAt: z.string(),
});
export type Project = z.infer<typeof projectSchema>;

/** What `project_detect` reports about a folder before it is added. */
export const projectDetectionSchema = z.object({
  path: z.string(),
  isGit: z.boolean(),
  currentBranch: z.string().nullable(),
  branches: z.array(z.string()),
  defaultBranch: z.string().nullable(),
  remote: z.string().nullable(),
});
export type ProjectDetection = z.infer<typeof projectDetectionSchema>;

/** Arguments to `project_add`. Every optional field falls back to what detection found. */
export const addProjectRequestSchema = z.object({
  path: z.string(),
  name: z.string().nullish(),
  defaultBranch: z.string().nullish(),
  remote: z.string().nullish(),
});
export type AddProjectRequest = z.infer<typeof addProjectRequestSchema>;

export const dirEntrySchema = z.object({
  name: z.string(),
  /** Forward-slash absolute path of the folder. */
  path: z.string(),
});
export type DirEntry = z.infer<typeof dirEntrySchema>;

export const dirListingSchema = z.object({
  /** Forward-slash absolute path of the listed folder. */
  path: z.string(),
  /** Parent folder, or null at a filesystem root. */
  parent: z.string().nullable(),
  dirs: z.array(dirEntrySchema),
});
export type DirListing = z.infer<typeof dirListingSchema>;

/** What a picker shortcut points at. */
export const fsRootKindSchema = z.enum(["home", "drive", "filesystem"]);
export type FsRootKind = z.infer<typeof fsRootKindSchema>;

export const fsRootSchema = z.object({
  /** Short chip label: `Home`, `C:`, or `/`. */
  name: z.string(),
  /** Forward-slash absolute path of the root. */
  path: z.string(),
  kind: fsRootKindSchema,
});
export type FsRoot = z.infer<typeof fsRootSchema>;

/** Every root the add-project picker offers; `home` is where a fresh picker opens. */
export const fsRootsSchema = z.object({
  home: z.string().nullable(),
  roots: z.array(fsRootSchema),
});
export type FsRoots = z.infer<typeof fsRootsSchema>;

/** Arguments to `fs_create_dir`: one new folder under an existing parent. */
export const createDirRequestSchema = z.object({
  parent: z.string(),
  name: z.string(),
});
export type CreateDirRequest = z.infer<typeof createDirRequestSchema>;
