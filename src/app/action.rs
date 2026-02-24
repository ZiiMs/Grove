use uuid::Uuid;

use crate::agent::{ProjectMgmtTaskStatus, StatusReason};
use crate::app::task_list::TaskListItem;
use crate::app::ToastLevel;

#[derive(Debug, Clone)]
pub enum Action {
    SelectNext,
    SelectPrevious,
    SelectFirst,
    SelectLast,

    CreateAgent {
        name: String,
        branch: String,
        task: Option<TaskListItem>,
    },
    DeleteAgent {
        id: Uuid,
    },
    AttachToAgent {
        id: Uuid,
    },
    AttachToDevServer {
        agent_id: Uuid,
    },
    DetachFromAgent,
    PauseAgent {
        id: Uuid,
    },
    ResumeAgent {
        id: Uuid,
    },

    UpdateAgentStatus {
        id: Uuid,
        status: crate::agent::AgentStatus,
        status_reason: Option<StatusReason>,
    },
    UpdateAgentOutput {
        id: Uuid,
        output: String,
    },
    SetAgentNote {
        id: Uuid,
        note: Option<String>,
    },

    RequestSummary {
        id: Uuid,
    },

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

    UpdateMrStatus {
        id: Uuid,
        status: crate::gitlab::MergeRequestStatus,
    },
    OpenMrInBrowser {
        id: Uuid,
    },
    OpenInEditor {
        id: Uuid,
    },

    UpdatePrStatus {
        id: Uuid,
        status: crate::github::PullRequestStatus,
    },
    OpenPrInBrowser {
        id: Uuid,
    },

    UpdateCodebergPrStatus {
        id: Uuid,
        status: crate::codeberg::PullRequestStatus,
    },
    OpenCodebergPrInBrowser {
        id: Uuid,
    },

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

    AssignProjectTask {
        id: Uuid,
        url_or_id: String,
    },
    UpdateProjectTaskStatus {
        id: Uuid,
        status: ProjectMgmtTaskStatus,
    },
    CycleTaskStatus {
        id: Uuid,
    },
    OpenTaskStatusDropdown {
        id: Uuid,
    },
    TaskStatusOptionsLoaded {
        id: Uuid,
        options: Vec<crate::app::StatusOption>,
    },
    TaskStatusDropdownNext,
    TaskStatusDropdownPrev,
    TaskStatusDropdownSelect,
    OpenProjectTaskInBrowser {
        id: Uuid,
    },
    DeleteAgentAndCompleteTask {
        id: Uuid,
    },

    FetchTaskList,
    RefreshTaskList,
    TaskListFetched {
        tasks: Vec<TaskListItem>,
    },
    TaskListFetchError {
        message: String,
    },
    SelectTaskNext,
    SelectTaskPrev,
    CreateAgentFromSelectedTask,
    AssignSelectedTaskToAgent,
    ToggleTaskExpand,
    ToggleSubtaskStatus,

    SubtaskStatusDropdownNext,
    SubtaskStatusDropdownPrev,
    SubtaskStatusDropdownSelect {
        completed: bool,
    },
    SubtaskStatusUpdated {
        task_id: String,
        completed: bool,
    },
    SubtaskStatusOptionsLoaded {
        task_id: String,
        task_name: String,
        options: Vec<crate::app::StatusOption>,
    },
    SubtaskStatusOptionSelected {
        task_id: String,
        status_name: String,
    },

    ConfirmTaskReassignment,
    DismissTaskReassignmentWarning,

    ToggleDiffView,
    ToggleHelp,
    ToggleLogs,
    ToggleStatusDebug,
    ShowError(String),
    ShowToast {
        message: String,
        level: ToastLevel,
    },
    ClearError,
    EnterInputMode(InputMode),
    ExitInputMode,
    UpdateInput(String),
    SubmitInput,

    RecordActivity {
        id: Uuid,
        had_activity: bool,
    },

    UpdateChecklistProgress {
        id: Uuid,
        progress: Option<(u32, u32)>,
    },

    UpdateGlobalSystemMetrics {
        cpu_percent: f32,
        memory_used: u64,
        memory_total: u64,
    },

    SetLoading(Option<String>),

    UpdatePreviewContent(Option<String>),

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

    CopyAgentName {
        id: Uuid,
    },

    RefreshAll,
    RefreshSelected,
    Tick,
    Quit,

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
    SettingsPromptSave,
    SettingsStartKeybindCapture,
    SettingsCaptureKeybind {
        key: String,
        modifiers: Vec<String>,
    },
    SettingsCancelKeybindCapture,
    SettingsDropdownPrev,
    SettingsDropdownNext,

    // File Browser
    SettingsCloseFileBrowser,
    FileBrowserToggle,
    FileBrowserSelectNext,
    FileBrowserSelectPrev,
    FileBrowserEnterDir,
    FileBrowserGoParent,

    // Settings Reset
    SettingsRequestReset {
        reset_type: crate::app::state::ResetType,
    },
    SettingsConfirmReset,
    SettingsCancelReset,

    // Global Setup Wizard
    GlobalSetupNextStep,
    GlobalSetupPrevStep,
    GlobalSetupSelectNext,
    GlobalSetupSelectPrev,
    GlobalSetupNavigateUp,
    GlobalSetupNavigateDown,
    GlobalSetupToggleDropdown,
    GlobalSetupDropdownPrev,
    GlobalSetupDropdownNext,
    GlobalSetupConfirmDropdown,
    GlobalSetupComplete,

    // Dev Server
    RequestStartDevServer,
    ConfirmStartDevServer,
    StartDevServer,
    StopDevServer,
    RestartDevServer,
    NextPreviewTab,
    PrevPreviewTab,
    ClearDevServerLogs,
    OpenDevServerInBrowser,
    DismissDevServerWarning,
    AppendDevServerLog {
        agent_id: Uuid,
        line: String,
    },
    UpdateDevServerStatus {
        agent_id: Uuid,
        status: crate::devserver::DevServerStatus,
    },

    // Project Setup Wizard
    ProjectSetupNavigateNext,
    ProjectSetupNavigatePrev,
    ProjectSetupSelect,
    ProjectSetupToggleDropdown,
    ProjectSetupDropdownPrev,
    ProjectSetupDropdownNext,
    ProjectSetupConfirmDropdown,
    ProjectSetupPmDropdownPrev,
    ProjectSetupPmDropdownNext,
    ProjectSetupConfirmPmDropdown,
    ProjectSetupSkip,
    ProjectSetupComplete,

    // PM Setup Wizard
    OpenPmSetup,
    ClosePmSetup,
    PmSetupNextStep,
    PmSetupPrevStep,
    PmSetupToggleAdvanced,
    PmSetupNavigateNext,
    PmSetupNavigatePrev,
    PmSetupToggleDropdown,
    PmSetupDropdownNext,
    PmSetupDropdownPrev,
    PmSetupConfirmDropdown,
    PmSetupInputChar(char),
    PmSetupBackspace,
    PmSetupTeamsLoaded {
        teams: Vec<(String, String, String)>,
    },
    PmSetupNotionDatabasesLoaded {
        databases: Vec<(String, String, String)>,
        parent_pages: Vec<(String, String, String)>,
    },
    PmSetupTeamsError {
        message: String,
    },
    PmSetupComplete,
    LinearUserFetched {
        username: String,
    },
    LinearUserFetchError {
        message: String,
    },

    // Git Setup Wizard
    OpenGitSetup,
    CloseGitSetup,
    GitSetupNextStep,
    GitSetupPrevStep,
    GitSetupToggleAdvanced,
    GitSetupNavigateNext,
    GitSetupNavigatePrev,
    GitSetupToggleDropdown,
    GitSetupDropdownNext,
    GitSetupDropdownPrev,
    GitSetupConfirmDropdown,
    GitSetupCloseDropdown,
    GitSetupInputChar(char),
    GitSetupBackspace,
    GitSetupStartEdit,
    GitSetupCancelEdit,
    GitSetupConfirmEdit,
    GitSetupFetchProjectId,
    GitSetupProjectIdFetched {
        id: u64,
        name: String,
    },
    GitSetupProjectIdError {
        message: String,
    },
    GitSetupComplete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    NewAgent,
    SetNote,
    ConfirmDelete,
    ConfirmMerge,
    ConfirmPush,
    ConfirmDeleteTask,
    AssignProjectTask,
    AssignAsana,
    ConfirmDeleteAsana,
    BrowseTasks,
    SelectTaskStatus,
    SelectSubtaskStatus,
}
