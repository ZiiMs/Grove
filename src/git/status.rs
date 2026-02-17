use serde::{Deserialize, Serialize};

/// Represents the git sync status of a worktree.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitSyncStatus {
    /// Commits ahead of remote tracking branch
    pub ahead: u32,
    /// Commits behind remote tracking branch
    pub behind: u32,
    /// Commits since branching from main
    pub divergence_from_main: u32,
    /// Whether the worktree is clean (no uncommitted changes)
    pub is_clean: bool,
    /// Whether we're synced with remote
    pub is_synced: bool,
}

impl GitSyncStatus {
    pub fn format_short(&self) -> String {
        format!(
            "↑{} ↓{} main+{}",
            self.ahead, self.behind, self.divergence_from_main
        )
    }
}
