use anyhow::{Context, Result};
use git2::Repository;
use std::path::{Path, PathBuf};

pub struct Worktree {
    repo_path: PathBuf,
    worktree_base: PathBuf,
}

impl Worktree {
    pub fn new(repo_path: &str, worktree_base: PathBuf) -> Self {
        Self {
            repo_path: PathBuf::from(repo_path),
            worktree_base,
        }
    }

    pub fn create(&self, branch: &str) -> Result<String> {
        let repo = Repository::open(&self.repo_path).context("Failed to open repository")?;

        if !self.worktree_base.exists() {
            std::fs::create_dir_all(&self.worktree_base)
                .context("Failed to create worktrees directory")?;
        }

        let worktree_path = self.worktree_base.join(branch.replace('/', "-"));
        let worktree_path_str = worktree_path.to_string_lossy().to_string();

        if worktree_path.exists() {
            return Ok(worktree_path_str);
        }

        let branch_ref = format!("refs/heads/{}", branch);
        let reference = match repo.find_reference(&branch_ref) {
            Ok(r) => r,
            Err(_) => {
                let head = repo.head().context("Failed to get HEAD")?;
                let commit = head.peel_to_commit().context("Failed to get HEAD commit")?;
                repo.branch(branch, &commit, false)
                    .context("Failed to create branch")?
                    .into_reference()
            }
        };

        repo.worktree(
            branch,
            &worktree_path,
            Some(git2::WorktreeAddOptions::new().reference(Some(&reference))),
        )
        .context("Failed to create worktree")?;

        Ok(worktree_path_str)
    }

    pub fn remove(&self, worktree_path: &str) -> Result<()> {
        let repo = Repository::open(&self.repo_path).context("Failed to open repository")?;

        let path = Path::new(worktree_path);
        let worktree_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .context("Invalid worktree path")?;

        if let Ok(wt) = repo.find_worktree(worktree_name) {
            wt.prune(Some(
                git2::WorktreePruneOptions::new()
                    .valid(true)
                    .working_tree(true),
            ))
            .context("Failed to prune worktree")?;
        }

        if path.exists() {
            std::fs::remove_dir_all(path).context("Failed to remove worktree directory")?;
        }

        Ok(())
    }

    pub fn list(&self) -> Result<Vec<String>> {
        let repo = Repository::open(&self.repo_path).context("Failed to open repository")?;

        let worktrees = repo.worktrees().context("Failed to list worktrees")?;

        Ok(worktrees.iter().flatten().map(String::from).collect())
    }

    pub fn exists(&self, branch: &str) -> bool {
        let worktree_path = self.worktree_base.join(branch.replace('/', "-"));
        worktree_path.exists()
    }

    pub fn worktree_path_for_branch(&self, branch: &str) -> PathBuf {
        self.worktree_base.join(branch.replace('/', "-"))
    }

    pub fn create_symlinks(&self, worktree_path: &str, files: &[String]) -> Result<()> {
        let worktree = Path::new(worktree_path);

        let is_in_home_dir = !worktree.starts_with(&self.repo_path);

        for file in files {
            let source = self.repo_path.join(file);
            let target = worktree.join(file);

            if !source.exists() {
                continue;
            }

            let target_is_broken_symlink = target
                .symlink_metadata()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
                && !target.exists();

            if target.exists() && !target_is_broken_symlink {
                continue;
            }

            if target_is_broken_symlink {
                let _ = std::fs::remove_file(&target);
            }

            if let Some(parent) = target.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("Failed to create directory {:?}", parent))?;
                }
            }

            let relative_source = if is_in_home_dir {
                pathdiff::diff_paths(&source, worktree).unwrap_or_else(|| source.clone())
            } else {
                Path::new("../..").join(file)
            };

            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&relative_source, &target).with_context(|| {
                    format!(
                        "Failed to create symlink from {:?} to {:?}",
                        target, relative_source
                    )
                })?;
            }
            #[cfg(windows)]
            {
                if source.is_dir() {
                    std::os::windows::fs::symlink_dir(&relative_source, &target).with_context(
                        || {
                            format!(
                                "Failed to create symlink from {:?} to {:?}",
                                target, relative_source
                            )
                        },
                    )?;
                } else {
                    std::os::windows::fs::symlink_file(&relative_source, &target).with_context(
                        || {
                            format!(
                                "Failed to create symlink from {:?} to {:?}",
                                target, relative_source
                            )
                        },
                    )?;
                }
            }
        }

        Ok(())
    }
}
