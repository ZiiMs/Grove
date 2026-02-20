pub mod client;
pub mod types;

pub use crate::ci::PipelineStatus;
pub use client::{GitLabClient, OptionalGitLabClient};
pub use types::{MergeRequestListItem, MergeRequestStatus};
