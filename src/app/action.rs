use uuid::Uuid;

/// All possible actions that can modify application state.
/// This provides a single point of control for state transitions.
#[derive(Debug, Clone)]
pub enum Action {
    // Navigation
    SelectNext,
    SelectPrevious,
    SelectFirst,
    SelectLast,

    // Agent lifecycle
    CreateAgent {
        name: String,
        branch: String,
    },
    DeleteAgent {
        id: Uuid,
    },
    AttachToAgent {
        id: Uuid,
    },
    DetachFromAgent,
    PauseAgent {
        id: Uuid,
    },
    ResumeAgent {
        id: Uuid,
    },

    // Agent status updates (from background tasks)
    UpdateAgentStatus {
        id: Uuid,
        status: crate::agent::AgentStatus,
    },
    UpdateAgentOutput {
        id: Uuid,
        output: String,
    },
    SetAgentNote {
        id: Uuid,
        note: Option<String>,
    },

    // Agent commands
    RequestSummary {
        id: Uuid,
    },

    // Git operations
    CheckoutBranch {
        id: Uuid,
    },
    MergeMain {
        id: Uuid,
    },
    PushBranch {
        id: Uuid,
    },
    FetchRemote {
        id: Uuid,
    },
    UpdateGitStatus {
        id: Uuid,
        status: crate::git::GitSyncStatus,
    },

    // GitLab operations
    UpdateMrStatus {
        id: Uuid,
        status: crate::gitlab::MergeRequestStatus,
    },
    OpenMrInBrowser {
        id: Uuid,
    },

    // GitHub operations
    UpdatePrStatus {
        id: Uuid,
        status: crate::github::PullRequestStatus,
    },
    OpenPrInBrowser {
        id: Uuid,
    },

    // Asana operations
    AssignAsanaTask {
        id: Uuid,
        url_or_gid: String,
    },
    UpdateAsanaTaskStatus {
        id: Uuid,
        status: crate::asana::AsanaTaskStatus,
    },
    OpenAsanaInBrowser {
        id: Uuid,
    },
    DeleteAgentAndCompleteAsana {
        id: Uuid,
    },

    // UI state
    ToggleDiffView,
    ToggleHelp,
    ToggleLogs,
    ShowError(String),
    ClearError,
    EnterInputMode(InputMode),
    ExitInputMode,
    UpdateInput(String),
    SubmitInput,

    // Activity tracking
    RecordActivity {
        id: Uuid,
        had_activity: bool,
    },

    // Checklist progress
    UpdateChecklistProgress {
        id: Uuid,
        progress: Option<(u32, u32)>,
    },

    // Global system metrics
    UpdateGlobalSystemMetrics {
        cpu_percent: f32,
        memory_used: u64,
        memory_total: u64,
    },

    // Loading state
    SetLoading(Option<String>),

    // Preview pane content
    UpdatePreviewContent(Option<String>),

    // Background task completions
    DeleteAgentComplete {
        id: Uuid,
        success: bool,
        message: String,
    },
    PauseAgentComplete {
        id: Uuid,
        success: bool,
        message: String,
    },
    ResumeAgentComplete {
        id: Uuid,
        success: bool,
        message: String,
    },

    // Clipboard
    CopyAgentName {
        id: Uuid,
    },

    // Application
    RefreshAll,
    Tick,
    Quit,

    // Settings
    ToggleSettings,
    SettingsSwitchSection,
    SettingsSwitchSectionBack,
    SettingsSelectNext,
    SettingsSelectPrev,
    SettingsSelectField,
    SettingsConfirmSelection,
    SettingsCancelSelection,
    SettingsInputChar(char),
    SettingsBackspace,
    SettingsClose,
    SettingsSave,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    NewAgent, // Just enter branch name, used as both name and branch
    SetNote,
    ConfirmDelete,
    ConfirmMerge,
    ConfirmPush,
    ConfirmDeleteAsana,
    AssignAsana,
}
