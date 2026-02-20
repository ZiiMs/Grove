use serde::{Deserialize, Serialize};

use crate::ci::PipelineStatus;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PullRequestStatus {
    #[default]
    None,
    Open {
        number: u64,
        url: String,
        pipeline: PipelineStatus,
    },
    Merged {
        number: u64,
    },
    Closed {
        number: u64,
    },
    Draft {
        number: u64,
        url: String,
        pipeline: PipelineStatus,
    },
}

impl PullRequestStatus {
    pub fn format_short(&self) -> String {
        match self {
            PullRequestStatus::None => "None".to_string(),
            PullRequestStatus::Open { number, .. } => format!("#{}", number),
            PullRequestStatus::Merged { number } => format!("#{} Merged", number),
            PullRequestStatus::Closed { number } => format!("#{} Closed", number),
            PullRequestStatus::Draft { number, .. } => format!("#{} Draft", number),
        }
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            PullRequestStatus::Open { url, .. } | PullRequestStatus::Draft { url, .. } => Some(url),
            _ => None,
        }
    }

    pub fn pipeline(&self) -> &PipelineStatus {
        match self {
            PullRequestStatus::Open { pipeline, .. }
            | PullRequestStatus::Draft { pipeline, .. } => pipeline,
            _ => &PipelineStatus::None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PullRequestResponse {
    pub number: u64,
    pub state: String,
    pub html_url: String,
    #[serde(default)]
    pub draft: bool,
    pub head: PullRequestHead,
    #[serde(default)]
    pub merged: bool,
    #[serde(rename = "merged_at")]
    pub merged_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PullRequestHead {
    #[serde(rename = "ref")]
    pub ref_field: String,
    #[allow(dead_code)]
    pub sha: String,
}
