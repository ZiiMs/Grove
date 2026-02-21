pub mod detector;
pub mod manager;
pub mod model;

pub use detector::{
    detect_checklist_progress, detect_mr_url, detect_status, detect_status_for_agent,
    detect_status_with_process, ForegroundProcess,
};
pub use manager::AgentManager;
pub use model::{Agent, AgentStatus, ProjectMgmtTaskStatus};
