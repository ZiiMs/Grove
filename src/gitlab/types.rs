use serde::{Deserialize, Serialize};

use crate::ci::PipelineStatus;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MergeRequestStatus {
    #[default]
    None,
    Open {
        iid: u64,
        url: String,
        pipeline: PipelineStatus,
    },
    Merged {
        iid: u64,
    },
    Conflicts {
        iid: u64,
        url: String,
        pipeline: PipelineStatus,
    },
    Approved {
        iid: u64,
        url: String,
        pipeline: PipelineStatus,
    },
    NeedsRebase {
        iid: u64,
        url: String,
        pipeline: PipelineStatus,
    },
}

impl MergeRequestStatus {
    pub fn format_short(&self) -> String {
        match self {
            MergeRequestStatus::None => "None".to_string(),
            MergeRequestStatus::Open { iid, .. } => format!("!{}", iid),
            MergeRequestStatus::Merged { iid } => format!("!{} Merged", iid),
            MergeRequestStatus::Conflicts { iid, .. } => format!("!{} Conflicts", iid),
            MergeRequestStatus::Approved { iid, .. } => format!("!{} Approved", iid),
            MergeRequestStatus::NeedsRebase { iid, .. } => format!("!{} Rebase", iid),
        }
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            MergeRequestStatus::Open { url, .. }
            | MergeRequestStatus::Conflicts { url, .. }
            | MergeRequestStatus::Approved { url, .. }
            | MergeRequestStatus::NeedsRebase { url, .. } => Some(url),
            _ => None,
        }
    }

    pub fn pipeline(&self) -> &PipelineStatus {
        match self {
            MergeRequestStatus::Open { pipeline, .. }
            | MergeRequestStatus::Conflicts { pipeline, .. }
            | MergeRequestStatus::Approved { pipeline, .. }
            | MergeRequestStatus::NeedsRebase { pipeline, .. } => pipeline,
            _ => &PipelineStatus::None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PipelineResponse {
    pub id: u64,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct MergeRequestListItem {
    pub iid: u64,
    pub web_url: String,
}

#[derive(Debug, Deserialize)]
pub struct MergeRequestResponse {
    pub iid: u64,
    pub state: String,
    pub web_url: String,
    pub has_conflicts: bool,
    pub approved: Option<bool>,
    pub source_branch: String,
    pub target_branch: String,
    pub head_pipeline: Option<PipelineResponse>,
    pub detailed_merge_status: Option<String>,
}

impl MergeRequestResponse {
    pub fn into_status(self) -> MergeRequestStatus {
        let pipeline = self
            .head_pipeline
            .map(|p| PipelineStatus::from_gitlab_status(&p.status))
            .unwrap_or_default();

        match self.state.as_str() {
            "merged" => MergeRequestStatus::Merged { iid: self.iid },
            "opened" => {
                let needs_rebase = self
                    .detailed_merge_status
                    .as_deref()
                    .map(|s| s == "need_rebase")
                    .unwrap_or(false);

                if self.has_conflicts {
                    MergeRequestStatus::Conflicts {
                        iid: self.iid,
                        url: self.web_url,
                        pipeline,
                    }
                } else if needs_rebase {
                    MergeRequestStatus::NeedsRebase {
                        iid: self.iid,
                        url: self.web_url,
                        pipeline,
                    }
                } else if self.approved.unwrap_or(false) {
                    MergeRequestStatus::Approved {
                        iid: self.iid,
                        url: self.web_url,
                        pipeline,
                    }
                } else {
                    MergeRequestStatus::Open {
                        iid: self.iid,
                        url: self.web_url,
                        pipeline,
                    }
                }
            }
            _ => MergeRequestStatus::None,
        }
    }
}
