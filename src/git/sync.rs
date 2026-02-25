use anyhow::{Context, Result};
use git2::{BranchType, Repository};
use std::process::Command;

use super::GitSyncStatus;

/// Git synchronization operations.
pub struct GitSync {
    worktree_path: String,
}

impl GitSync {
    pub fn new(worktree_path: &str) -> Self {
        Self {
            worktree_path: worktree_path.to_string(),
        }
    }

    /// Get the sync status of the worktree.
    pub fn get_status(&self, main_branch: &str) -> Result<GitSyncStatus> {
        let repo =
            Repository::open(&self.worktree_path).context("Failed to open worktree repository")?;

        let head = repo.head().context("Failed to get HEAD")?;
        let head_commit = head.peel_to_commit().context("Failed to get HEAD commit")?;

        // Get ahead/behind from remote tracking branch
        let (ahead, behind) = self.get_ahead_behind(&repo)?;

        // Get divergence from main
        let divergence_from_main =
            self.get_divergence_from_main(&repo, main_branch, &head_commit)?;

        // Check if working tree is clean
        let is_clean = self.is_clean(&repo)?;

        Ok(GitSyncStatus {
            ahead,
            behind,
            divergence_from_main,
            is_clean,
            is_synced: ahead == 0 && behind == 0,
        })
    }

    fn get_ahead_behind(&self, repo: &Repository) -> Result<(u32, u32)> {
        let head = repo.head()?;

        if !head.is_branch() {
            return Ok((0, 0));
        }

        let branch_name = head.shorthand().context("Failed to get branch name")?;

        // Try to find upstream
        let branch = repo.find_branch(branch_name, BranchType::Local)?;

        let upstream = match branch.upstream() {
            Ok(u) => u,
            Err(_) => return Ok((0, 0)), // No upstream set
        };

        let local_oid = head.target().context("Failed to get local OID")?;
        let upstream_oid = upstream
            .get()
            .target()
            .context("Failed to get upstream OID")?;

        let (ahead, behind) = repo.graph_ahead_behind(local_oid, upstream_oid)?;

        Ok((ahead as u32, behind as u32))
    }

    fn get_divergence_from_main(
        &self,
        repo: &Repository,
        main_branch: &str,
        head_commit: &git2::Commit,
    ) -> Result<u32> {
        // Try to find main branch
        let main_ref = format!("refs/heads/{}", main_branch);
        let main_branch = match repo.find_reference(&main_ref) {
            Ok(r) => r,
            Err(_) => {
                // Try origin/main
                let remote_ref = format!("refs/remotes/origin/{}", main_branch);
                match repo.find_reference(&remote_ref) {
                    Ok(r) => r,
                    Err(_) => return Ok(0),
                }
            }
        };

        let main_oid = main_branch.target().context("Failed to get main OID")?;
        let head_oid = head_commit.id();

        // Find merge base
        let merge_base = match repo.merge_base(main_oid, head_oid) {
            Ok(mb) => mb,
            Err(_) => return Ok(0),
        };

        // Count commits from merge base to HEAD
        let mut revwalk = repo.revwalk()?;
        revwalk.push(head_oid)?;
        revwalk.hide(merge_base)?;

        let count = revwalk.count();
        Ok(count as u32)
    }

    fn is_clean(&self, repo: &Repository) -> Result<bool> {
        let statuses = repo.statuses(Some(
            git2::StatusOptions::new()
                .include_untracked(true)
                .recurse_untracked_dirs(true),
        ))?;

        Ok(statuses.is_empty())
    }

    /// Fetch from remote.
    pub fn fetch(&self) -> Result<()> {
        let output = Command::new("git")
            .args(["fetch", "--all", "--prune"])
            .current_dir(&self.worktree_path)
            .output()
            .context("Failed to execute git fetch")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Git fetch failed: {}", stderr);
        }

        Ok(())
    }

    /// Merge main branch into the current worktree.
    pub fn merge_main(&self, main_branch: &str) -> Result<()> {
        // First fetch
        self.fetch()?;

        // Then merge
        let output = Command::new("git")
            .args(["merge", &format!("origin/{}", main_branch), "--no-edit"])
            .current_dir(&self.worktree_path)
            .output()
            .context("Failed to execute git merge")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("CONFLICT") {
                anyhow::bail!("Merge conflict detected. Please resolve manually.");
            }
            anyhow::bail!("Git merge failed: {}", stderr);
        }

        Ok(())
    }

    /// Checkout a branch in the worktree.
    pub fn checkout(&self, branch: &str) -> Result<()> {
        let output = Command::new("git")
            .args(["checkout", branch])
            .current_dir(&self.worktree_path)
            .output()
            .context("Failed to execute git checkout")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Git checkout failed: {}", stderr);
        }

        Ok(())
    }

    /// Get the current branch name.
    pub fn current_branch(&self) -> Result<String> {
        let repo = Repository::open(&self.worktree_path)?;
        let head = repo.head()?;

        head.shorthand()
            .map(String::from)
            .context("Failed to get branch name")
    }

    /// Get diff output.
    pub fn get_diff(&self) -> Result<String> {
        let output = Command::new("git")
            .args(["diff", "--color=never"])
            .current_dir(&self.worktree_path)
            .output()
            .context("Failed to execute git diff")?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Get diff against main branch.
    pub fn get_diff_against_main(&self, main_branch: &str) -> Result<String> {
        let output = Command::new("git")
            .args([
                "diff",
                "--color=never",
                &format!("origin/{}...HEAD", main_branch),
            ])
            .current_dir(&self.worktree_path)
            .output()
            .context("Failed to execute git diff")?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}
