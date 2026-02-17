use anyhow::{Context, Result};
use git2::Repository;
use std::path::{Path, PathBuf};

/// Manages git worktrees for agent isolation.
pub struct Worktree {
    repo_path: PathBuf,
}

impl Worktree {
    pub fn new(repo_path: &str) -> Self {
        Self {
            repo_path: PathBuf::from(repo_path),
        }
    }

    /// Create a new worktree for a branch.
    /// Returns the path to the created worktree.
    pub fn create(&self, branch: &str) -> Result<String> {
        let repo = Repository::open(&self.repo_path).context("Failed to open repository")?;

        // Determine worktree path
        let worktrees_dir = self.repo_path.join(".worktrees");
        if !worktrees_dir.exists() {
            std::fs::create_dir_all(&worktrees_dir)
                .context("Failed to create worktrees directory")?;
        }

        let worktree_path = worktrees_dir.join(branch.replace('/', "-"));
        let worktree_path_str = worktree_path.to_string_lossy().to_string();

        // Check if worktree already exists
        if worktree_path.exists() {
            return Ok(worktree_path_str);
        }

        // Try to find the branch
        let branch_ref = format!("refs/heads/{}", branch);
        let reference = match repo.find_reference(&branch_ref) {
            Ok(r) => r,
            Err(_) => {
                // Branch doesn't exist, create it from HEAD
                let head = repo.head().context("Failed to get HEAD")?;
                let commit = head.peel_to_commit().context("Failed to get HEAD commit")?;
                repo.branch(branch, &commit, false)
                    .context("Failed to create branch")?
                    .into_reference()
            }
        };

        // Create the worktree
        repo.worktree(
            branch,
            &worktree_path,
            Some(git2::WorktreeAddOptions::new().reference(Some(&reference))),
        )
        .context("Failed to create worktree")?;

        Ok(worktree_path_str)
    }

    /// Remove a worktree.
    pub fn remove(&self, worktree_path: &str) -> Result<()> {
        let repo = Repository::open(&self.repo_path).context("Failed to open repository")?;

        let path = Path::new(worktree_path);
        let worktree_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .context("Invalid worktree path")?;

        // Find and remove the worktree
        if let Ok(wt) = repo.find_worktree(worktree_name) {
            // Prune the worktree (removes even if dirty)
            wt.prune(Some(
                git2::WorktreePruneOptions::new()
                    .valid(true)
                    .working_tree(true),
            ))
            .context("Failed to prune worktree")?;
        }

        // Remove the directory if it still exists
        if path.exists() {
            std::fs::remove_dir_all(path).context("Failed to remove worktree directory")?;
        }

        Ok(())
    }

    /// List all worktrees.
    pub fn list(&self) -> Result<Vec<String>> {
        let repo = Repository::open(&self.repo_path).context("Failed to open repository")?;

        let worktrees = repo.worktrees().context("Failed to list worktrees")?;

        Ok(worktrees.iter().flatten().map(String::from).collect())
    }

    /// Check if a worktree exists for a branch.
    pub fn exists(&self, branch: &str) -> bool {
        let worktrees_dir = self.repo_path.join(".worktrees");
        let worktree_path = worktrees_dir.join(branch.replace('/', "-"));
        worktree_path.exists()
    }

    /// Get the path where a worktree would be created for a branch.
    pub fn worktree_path_for_branch(&self, branch: &str) -> PathBuf {
        self.repo_path
            .join(".worktrees")
            .join(branch.replace('/', "-"))
    }
}
