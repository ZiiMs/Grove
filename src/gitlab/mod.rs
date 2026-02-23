pub mod client;
pub mod types;

pub use crate::ci::PipelineStatus;
pub use client::{fetch_project_by_path, GitLabClient, OptionalGitLabClient};
pub use types::{MergeRequestListItem, MergeRequestStatus};
