//! Unified diffs: the working tree, the index, or a branch against a base.
//!
//! The patch is bounded by [`MAX_OUTPUT_BYTES`](super::MAX_OUTPUT_BYTES); when it is cut, the text ends
//! with a marker and `truncated` is set, so the UI never silently shows half a diff as if it were whole.

use std::path::Path;

use super::{
    git_failure, truncate_output, validate_path, validate_ref, DiffRequest, DiffTarget, Git,
    GitDiff, MAX_OUTPUT_BYTES,
};
use crate::ipc::IpcResult;
use crate::paths::display_path;

impl Git {
    pub fn diff(&self, repo: Option<&Path>, request: &DiffRequest) -> IpcResult<GitDiff> {
        let root = self.discover(repo)?;

        let path = match &request.path {
            Some(path) => {
                validate_path(path)?;
                Some(path.clone())
            }
            None => None,
        };
        let (base, branch) = match request.target {
            DiffTarget::Branch => {
                let base = validate_ref("base", request.base.as_deref().unwrap_or_default())?;
                let branch = validate_ref("branch", request.branch.as_deref().unwrap_or_default())?;
                (Some(base), Some(branch))
            }
            DiffTarget::WorkingTree | DiffTarget::Staged => (None, None),
        };

        let has_head = self.head_exists(&root)?;
        let mut args = vec![
            "diff".to_string(),
            "--no-color".to_string(),
            "--no-ext-diff".to_string(),
            "--no-textconv".to_string(),
            "--unified=3".to_string(),
        ];
        match request.target {
            DiffTarget::WorkingTree => {
                if has_head {
                    args.push("HEAD".to_string());
                }
            }
            DiffTarget::Staged => {
                args.push("--cached".to_string());
                if has_head {
                    args.push("HEAD".to_string());
                }
            }
            DiffTarget::Branch => {
                let base = base.as_deref().unwrap_or_default();
                let branch = branch.as_deref().unwrap_or_default();
                args.push(format!("{base}...{branch}"));
            }
        }
        // `--` makes a path that starts with `-` a path, never an option.
        args.push("--".to_string());
        if let Some(path) = &path {
            args.push(path.clone());
        }

        let output = self.run_args(&root, &args)?;
        if !output.success() {
            return Err(git_failure("diff", &output));
        }
        let (patch, truncated) = truncate_output(&output.stdout, MAX_OUTPUT_BYTES);

        Ok(GitDiff {
            repo_root: display_path(&root),
            target: request.target,
            path,
            base,
            branch,
            patch,
            truncated,
        })
    }

    fn head_exists(&self, root: &Path) -> IpcResult<bool> {
        let output = self.run(root, &["rev-parse", "--verify", "--quiet", "HEAD"])?;
        Ok(output.success())
    }
}
