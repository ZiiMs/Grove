//! The wire types gitops returns. Rust owns the shape; `packages/contract/src/git.ts` mirrors it in
//! zod and both sides are pinned by `contract/fixtures/git-*.json`.
//!
//! Paths cross the wire in the forward-slash form from [`crate::paths::display_path`], so a fixture is
//! identical on Windows and CI.

use serde::{Deserialize, Serialize};

/// One entry of `git status --porcelain`: the two status characters plus where the change lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusFile {
    /// Path relative to the repository root, forward slashes.
    pub path: String,
    /// Previous path for a rename or copy, otherwise `null`.
    pub from: Option<String>,
    /// The index (staged) status character, e.g. `"M"`, `"A"`, `"?"`.
    pub index: String,
    /// The worktree (unstaged) status character, e.g. `"M"`, `" "`, `"?"`.
    pub worktree: String,
    pub staged: bool,
    pub untracked: bool,
    /// Lines added by this change. An untracked file counts as all added.
    pub added: u32,
    /// Lines removed by this change; always `0` for an untracked file.
    pub removed: u32,
}

/// Working tree state for one repository. Fixture: `contract/fixtures/git-status.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub repo_root: String,
    /// Current branch, or `null` on a detached HEAD.
    pub branch: Option<String>,
    /// Full commit id of HEAD, or `null` before the first commit.
    pub head: Option<String>,
    pub clean: bool,
    /// True when the status output exceeded [`super::MAX_OUTPUT_BYTES`] and was cut with a marker.
    pub truncated: bool,
    pub files: Vec<GitStatusFile>,
}

/// One local or remote-tracking branch. Fixture: `contract/fixtures/git-branch.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBranch {
    /// Local name (`main`) or remote-tracking name (`origin/main`).
    pub name: String,
    /// Remote name for a remote-tracking branch, `null` for a local branch.
    pub remote: Option<String>,
    pub current: bool,
    /// Configured upstream, e.g. `origin/main`.
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

/// One worktree of the repository. Fixture: `contract/fixtures/git-worktree.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitWorktree {
    pub path: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    /// True for the worktree the panel is looking at.
    pub current: bool,
}

/// What a [`GitDiff`] compares.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffTarget {
    /// Uncommitted changes against HEAD (or the index before the first commit).
    #[default]
    WorkingTree,
    /// The index against HEAD.
    Staged,
    /// A branch against a base branch (`base...branch`, the merge-base form).
    Branch,
}

/// A unified diff. Fixture: `contract/fixtures/git-diff.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiff {
    pub repo_root: String,
    pub target: DiffTarget,
    /// Restrict the diff to one repository-relative path, or `null` for everything.
    pub path: Option<String>,
    pub base: Option<String>,
    pub branch: Option<String>,
    pub patch: String,
    /// True when the patch exceeded [`super::MAX_OUTPUT_BYTES`]; `patch` then ends with a marker.
    pub truncated: bool,
}

/// A draft pull request created by `gh`. Fixture: `contract/fixtures/git-pr.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrResult {
    pub url: String,
    pub draft: bool,
    pub base: Option<String>,
}

/// What the webview asks [`super::Git::diff`] for. Built from command arguments, not from the wire.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffRequest {
    pub target: DiffTarget,
    pub path: Option<String>,
    pub base: Option<String>,
    pub branch: Option<String>,
}

impl DiffRequest {
    pub fn working_tree(path: Option<String>) -> Self {
        Self {
            target: DiffTarget::WorkingTree,
            path,
            ..Self::default()
        }
    }

    pub fn staged(path: Option<String>) -> Self {
        Self {
            target: DiffTarget::Staged,
            path,
            ..Self::default()
        }
    }

    /// Diff `branch` against `base` using `git diff base...branch`.
    pub fn branch(
        base: impl Into<String>,
        branch: impl Into<String>,
        path: Option<String>,
    ) -> Self {
        Self {
            target: DiffTarget::Branch,
            path,
            base: Some(base.into()),
            branch: Some(branch.into()),
        }
    }

    /// Build from the command surface. A branch diff needs both `base` and `branch`; `staged` is a
    /// separate mode and cannot be combined with them.
    pub fn from_parts(
        path: Option<String>,
        branch: Option<String>,
        base: Option<String>,
        staged: bool,
    ) -> crate::ipc::IpcResult<Self> {
        match (branch, base) {
            (Some(branch), Some(base)) => {
                if staged {
                    return Err(crate::ipc::IpcError::invalid(
                        "a staged diff cannot also compare two branches",
                    ));
                }
                Ok(Self::branch(base, branch, path))
            }
            (None, None) => Ok(if staged {
                Self::staged(path)
            } else {
                Self::working_tree(path)
            }),
            _ => Err(crate::ipc::IpcError::invalid(
                "a branch diff needs both a base branch and a branch",
            )),
        }
    }
}

/// What detection reports about a folder before it becomes a project. This is the source data for
/// the IPC `ProjectDetection`; it stays in gitops because only git knows the answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDetection {
    pub is_git: bool,
    pub current_branch: Option<String>,
    pub branches: Vec<String>,
    pub default_branch: Option<String>,
    pub remote: Option<String>,
}

/// What the webview asks [`super::Git::open_pr`] for. Built from command arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrRequest {
    pub title: String,
    pub body: String,
    pub base: Option<String>,
    pub head: Option<String>,
    pub draft: bool,
}

impl PrRequest {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            base: None,
            head: None,
            draft: false,
        }
    }

    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.base = Some(base.into());
        self
    }

    pub fn with_head(mut self, head: impl Into<String>) -> Self {
        self.head = Some(head.into());
        self
    }

    pub fn with_draft(mut self, draft: bool) -> Self {
        self.draft = draft;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_requests_map_command_arguments_onto_targets() {
        let working = DiffRequest::from_parts(Some("a.txt".into()), None, None, false).unwrap();
        assert_eq!(working.target, DiffTarget::WorkingTree);

        let staged = DiffRequest::from_parts(None, None, None, true).unwrap();
        assert_eq!(staged.target, DiffTarget::Staged);

        let branch =
            DiffRequest::from_parts(None, Some("feature".into()), Some("main".into()), false)
                .unwrap();
        assert_eq!(branch.target, DiffTarget::Branch);
        assert_eq!(branch.base.as_deref(), Some("main"));
        assert_eq!(branch.branch.as_deref(), Some("feature"));

        assert!(DiffRequest::from_parts(None, Some("feature".into()), None, false).is_err());
        assert!(DiffRequest::from_parts(None, None, Some("main".into()), false).is_err());
        assert!(
            DiffRequest::from_parts(None, Some("feature".into()), Some("main".into()), true)
                .is_err()
        );
    }
}
