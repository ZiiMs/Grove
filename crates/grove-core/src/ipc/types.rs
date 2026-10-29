use crate::providers::{PermissionMode, ThinkingLevel};
use crate::sessions::event::{SessionEvent, SessionState, TitleSource};
use serde::{Deserialize, Serialize};

/// Header payload for the shell. Pinned by `contract/fixtures/app-info.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    /// Forward-slash path, so the fixture is identical on Windows and CI.
    pub app_data_dir: String,
    pub platform: String,
}

/// Envelope for every `grove://` event. `payload` stays generic at the wire level; the webview
/// narrows it with a zod schema and ignores kinds it does not know.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub payload: T,
}

impl<T> EventEnvelope<T> {
    pub fn global(payload: T) -> Self {
        Self {
            session_id: None,
            payload,
        }
    }

    pub fn session(session_id: impl Into<String>, payload: T) -> Self {
        Self {
            session_id: Some(session_id.into()),
            payload,
        }
    }
}

/// A session row: the durable identity of a conversation. A session is a row, not a process, which
/// is what makes restart survival possible (see `architecture.md` §Session runtime).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    /// The provider this session talks to (`anthropic`, `openai`, `google`, `openrouter`).
    pub provider: String,
    /// Forward-slash working directory the agent runs in.
    pub cwd: String,
    pub model: Option<String>,
    pub title: String,
    /// Who wrote the title: the user (`manual`), the first-message placeholder (`auto`) or a model
    /// (`generated`). Only an `auto` session still carrying the temporary title is eligible for
    /// automatic replacement.
    pub title_source: TitleSource,
    pub status: SessionState,
    /// The workspace project this session belongs to, or `None` for a loose chat.
    pub project_id: Option<String>,
    /// True while a permission request is waiting for the user.
    pub awaiting: bool,
    /// When the session was archived, or `None` while it is active. Archive is a nullable column,
    /// not a delete: the row, its transcript and its plan stay exactly where they were.
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One `subagent` call in a session, as the tree shows it. The worker's conversation is ephemeral;
/// this row is the durable trace of that it ran, what it was asked to do, and how it ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentSummary {
    /// The parent's tool-call id, which also prefixes the worker's forwarded transcript rows.
    pub call_id: String,
    pub description: String,
    /// `running`, then the tool result's `ok`, `error` or `cancelled`.
    pub status: String,
    /// The stored tool-result summary; the full report is only in the parent model's context.
    pub summary: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

/// One row of the session list: the session plus the transcript size, so the list needs no join in
/// the webview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    #[serde(flatten)]
    pub session: Session,
    pub event_count: u64,
    /// Delegations started from this session, oldest first. Empty for a session that never used one.
    #[serde(default)]
    pub subagents: Vec<SubagentSummary>,
}

/// A session and its full transcript in sequence order. `session_resume` restarts it from the row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    #[serde(flatten)]
    pub session: Session,
    /// The reasoning level the runtime will use on the next turn.
    pub thinking_level: ThinkingLevel,
    /// Whether mutating tools ask before they run.
    #[serde(default)]
    pub permission_mode: PermissionMode,
    pub events: Vec<SessionEvent>,
}

/// Arguments to `session_create`. `cwd` falls back to the process directory, then the app data dir.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    /// The provider to run the session on. Empty means "the configured default provider".
    pub provider: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// The project this session belongs to; when set, `cwd` resolves from it.
    #[serde(default)]
    pub project_id: Option<String>,
    /// A worktree checkout to run in; only meaningful with `project_id`.
    #[serde(default)]
    pub worktree_path: Option<String>,
    /// provider config from settings. The runtime reads `maxSteps` from here.
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}

/// A workspace project: a folder the user added, with the git facts the tree needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    /// Forward-slash absolute path, as the user picked it.
    pub path: String,
    /// `owner/name`, or `None` when the folder has no git remote.
    pub remote: Option<String>,
    /// Branch new sessions branch from / PRs target; `None` for a non-git folder.
    pub default_branch: Option<String>,
    pub is_git: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// What detection reports about a folder before it becomes a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetection {
    pub path: String,
    pub is_git: bool,
    pub current_branch: Option<String>,
    pub branches: Vec<String>,
    pub default_branch: Option<String>,
    pub remote: Option<String>,
}

/// One folder in the add-project picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    /// Forward-slash absolute path of the folder.
    pub path: String,
}

/// One level of directories, for the add-project picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    /// Forward-slash absolute path of the listed folder.
    pub path: String,
    /// Parent folder, or `None` at a filesystem root.
    pub parent: Option<String>,
    pub dirs: Vec<DirEntry>,
}

/// What a picker shortcut points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsRootKind {
    Home,
    Drive,
    Filesystem,
}

/// One place the add-project picker can jump to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsRoot {
    pub name: String,
    /// Forward-slash display path.
    pub path: String,
    pub kind: FsRootKind,
}

/// Every root the picker offers. `home` is where a fresh picker opens; `roots` is the chip list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsRoots {
    pub home: Option<String>,
    pub roots: Vec<FsRoot>,
}
