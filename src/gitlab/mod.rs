pub mod client;
pub mod types;

pub use client::{GitLabClient, OptionalGitLabClient};
pub use types::{MergeRequestListItem, MergeRequestStatus, PipelineStatus};
