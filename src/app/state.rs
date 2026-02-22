use chrono::Utc;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use uuid::Uuid;

use super::action::InputMode;
use super::config::{
    AiAgent, Config, GitProvider, Keybind, Keybinds, LogLevel as ConfigLogLevel,
    ProjectMgmtProvider, RepoConfig, UiConfig, WorktreeLocation,
};
use super::task_list::TaskListItem;
use crate::agent::Agent;
use crate::ui::components::file_browser::DirEntry;

const SYSTEM_METRICS_HISTORY_SIZE: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewTab {
    #[default]
    Preview,
    DevServer,
}

#[derive(Debug, Clone)]
pub struct DevServerWarning {
    pub agent_id: Uuid,
    pub running_servers: Vec<(String, Option<u16>)>,
}

#[derive(Debug, Clone)]
pub struct TaskReassignmentWarning {
    pub target_agent_id: Uuid,
    pub task_id: String,
    pub task_name: String,
    pub agent_current_task: Option<(String, String)>,
    pub task_current_agent: Option<(Uuid, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Git,
    ProjectMgmt,
    DevServer,
    Keybinds,
}

impl SettingsTab {
    pub fn all() -> &'static [SettingsTab] {
        &[
            SettingsTab::General,
            SettingsTab::Git,
            SettingsTab::ProjectMgmt,
            SettingsTab::DevServer,
            SettingsTab::Keybinds,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            SettingsTab::General => "General",
            SettingsTab::Git => "Git",
            SettingsTab::ProjectMgmt => "Project Mgmt",
            SettingsTab::DevServer => "Dev Server",
            SettingsTab::Keybinds => "Keybinds",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            SettingsTab::General => SettingsTab::Git,
            SettingsTab::Git => SettingsTab::ProjectMgmt,
            SettingsTab::ProjectMgmt => SettingsTab::DevServer,
            SettingsTab::DevServer => SettingsTab::Keybinds,
            SettingsTab::Keybinds => SettingsTab::General,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            SettingsTab::General => SettingsTab::Keybinds,
            SettingsTab::Git => SettingsTab::General,
            SettingsTab::ProjectMgmt => SettingsTab::Git,
            SettingsTab::DevServer => SettingsTab::ProjectMgmt,
            SettingsTab::Keybinds => SettingsTab::DevServer,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    AiAgent,
    Editor,
    LogLevel,
    WorktreeLocation,
    ShowPreview,
    ShowMetrics,
    ShowLogs,
    ShowBanner,
    GitProvider,
    GitLabProjectId,
    GitLabBaseUrl,
    GitHubOwner,
    GitHubRepo,
    CodebergOwner,
    CodebergRepo,
    CodebergBaseUrl,
    CodebergCiProvider,
    BranchPrefix,
    MainBranch,
    WorktreeSymlinks,
    ProjectMgmtProvider,
    AsanaProjectGid,
    AsanaInProgressGid,
    AsanaDoneGid,
    NotionDatabaseId,
    NotionStatusProperty,
    NotionInProgressOption,
    NotionDoneOption,
    ClickUpListId,
    ClickUpInProgressStatus,
    ClickUpDoneStatus,
    SummaryPrompt,
    MergePrompt,
    PushPrompt,
    DevServerCommand,
    DevServerRunBefore,
    DevServerWorkingDir,
    DevServerPort,
    DevServerAutoStart,
    KbNavDown,
    KbNavUp,
    KbNavFirst,
    KbNavLast,
    KbNewAgent,
    KbDeleteAgent,
    KbAttach,
    KbSetNote,
    KbYank,
    KbPause,
    KbResume,
    KbMerge,
    KbPush,
    KbFetch,
    KbSummary,
    KbToggleDiff,
    KbToggleLogs,
    KbOpenMr,
    KbAsanaAssign,
    KbAsanaOpen,
    KbRefreshAll,
    KbToggleHelp,
    KbToggleSettings,
    KbQuit,
    KbOpenEditor,
    KbShowTasks,
    KbRefreshTaskList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCategory {
    Agent,
    Display,
    Storage,
    GitProvider,
    GitConfig,
    Ci,
    ProjectMgmt,
    Asana,
    Notion,
    Clickup,
    Prompts,
    DevServer,
    KeybindNav,
    KeybindAgent,
    KeybindGit,
    KeybindExternal,
    KeybindOther,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionButtonType {
    ResetTab,
    ResetAll,
}

impl ActionButtonType {
    pub fn display_name(&self) -> &'static str {
        match self {
            ActionButtonType::ResetTab => "Reset Tab to Defaults",
            ActionButtonType::ResetAll => "Reset All Settings",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetType {
    CurrentTab,
    AllSettings,
}

impl SettingsCategory {
    pub fn display_name(&self) -> &'static str {
        match self {
            SettingsCategory::Agent => "Agent",
            SettingsCategory::Display => "Display",
            SettingsCategory::Storage => "Storage",
            SettingsCategory::GitProvider => "Provider",
            SettingsCategory::GitConfig => "Configuration",
            SettingsCategory::Ci => "CI/CD",
            SettingsCategory::ProjectMgmt => "Project Mgmt",
            SettingsCategory::Asana => "Asana",
            SettingsCategory::Notion => "Notion",
            SettingsCategory::Clickup => "ClickUp",
            SettingsCategory::Prompts => "Prompts",
            SettingsCategory::DevServer => "Dev Server",
            SettingsCategory::KeybindNav => "Navigation",
            SettingsCategory::KeybindAgent => "Agent Management",
            SettingsCategory::KeybindGit => "Git Operations",
            SettingsCategory::KeybindExternal => "External Services",
            SettingsCategory::KeybindOther => "Other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsItem {
    Category(SettingsCategory),
    Field(SettingsField),
    ActionButton(ActionButtonType),
}

impl SettingsField {
    pub fn tab(&self) -> SettingsTab {
        match self {
            SettingsField::AiAgent
            | SettingsField::Editor
            | SettingsField::LogLevel
            | SettingsField::WorktreeLocation
            | SettingsField::ShowPreview
            | SettingsField::ShowMetrics
            | SettingsField::ShowLogs
            | SettingsField::ShowBanner
            | SettingsField::SummaryPrompt
            | SettingsField::MergePrompt
            | SettingsField::PushPrompt => SettingsTab::General,
            SettingsField::GitProvider
            | SettingsField::GitLabProjectId
            | SettingsField::GitLabBaseUrl
            | SettingsField::GitHubOwner
            | SettingsField::GitHubRepo
            | SettingsField::CodebergOwner
            | SettingsField::CodebergRepo
            | SettingsField::CodebergBaseUrl
            | SettingsField::CodebergCiProvider
            | SettingsField::BranchPrefix
            | SettingsField::MainBranch => SettingsTab::Git,
            SettingsField::ProjectMgmtProvider
            | SettingsField::AsanaProjectGid
            | SettingsField::AsanaInProgressGid
            | SettingsField::AsanaDoneGid
            | SettingsField::NotionDatabaseId
            | SettingsField::NotionStatusProperty
            | SettingsField::NotionInProgressOption
            | SettingsField::NotionDoneOption
            | SettingsField::ClickUpListId
            | SettingsField::ClickUpInProgressStatus
            | SettingsField::ClickUpDoneStatus => SettingsTab::ProjectMgmt,
            SettingsField::DevServerCommand
            | SettingsField::DevServerRunBefore
            | SettingsField::DevServerWorkingDir
            | SettingsField::DevServerPort
            | SettingsField::DevServerAutoStart
            | SettingsField::WorktreeSymlinks => SettingsTab::DevServer,
            SettingsField::KbNavDown
            | SettingsField::KbNavUp
            | SettingsField::KbNavFirst
            | SettingsField::KbNavLast
            | SettingsField::KbNewAgent
            | SettingsField::KbDeleteAgent
            | SettingsField::KbAttach
            | SettingsField::KbSetNote
            | SettingsField::KbYank
            | SettingsField::KbPause
            | SettingsField::KbResume
            | SettingsField::KbMerge
            | SettingsField::KbPush
            | SettingsField::KbFetch
            | SettingsField::KbSummary
            | SettingsField::KbToggleDiff
            | SettingsField::KbToggleLogs
            | SettingsField::KbOpenMr
            | SettingsField::KbAsanaAssign
            | SettingsField::KbAsanaOpen
            | SettingsField::KbRefreshAll
            | SettingsField::KbToggleHelp
            | SettingsField::KbToggleSettings
            | SettingsField::KbQuit
            | SettingsField::KbOpenEditor
            | SettingsField::KbShowTasks
            | SettingsField::KbRefreshTaskList => SettingsTab::Keybinds,
        }
    }

    pub fn is_prompt_field(&self) -> bool {
        matches!(
            self,
            SettingsField::SummaryPrompt | SettingsField::MergePrompt | SettingsField::PushPrompt
        )
    }

    pub fn is_keybind_field(&self) -> bool {
        matches!(
            self,
            SettingsField::KbNavDown
                | SettingsField::KbNavUp
                | SettingsField::KbNavFirst
                | SettingsField::KbNavLast
                | SettingsField::KbNewAgent
                | SettingsField::KbDeleteAgent
                | SettingsField::KbAttach
                | SettingsField::KbSetNote
                | SettingsField::KbYank
                | SettingsField::KbPause
                | SettingsField::KbResume
                | SettingsField::KbMerge
                | SettingsField::KbPush
                | SettingsField::KbFetch
                | SettingsField::KbSummary
                | SettingsField::KbToggleDiff
                | SettingsField::KbToggleLogs
                | SettingsField::KbOpenMr
                | SettingsField::KbAsanaAssign
                | SettingsField::KbAsanaOpen
                | SettingsField::KbRefreshAll
                | SettingsField::KbToggleHelp
                | SettingsField::KbToggleSettings
                | SettingsField::KbQuit
                | SettingsField::KbOpenEditor
                | SettingsField::KbShowTasks
                | SettingsField::KbRefreshTaskList
        )
    }

    pub fn keybind_name(&self) -> Option<&'static str> {
        match self {
            SettingsField::KbNavDown => Some("Move Down"),
            SettingsField::KbNavUp => Some("Move Up"),
            SettingsField::KbNavFirst => Some("Go to First"),
            SettingsField::KbNavLast => Some("Go to Last"),
            SettingsField::KbNewAgent => Some("New Agent"),
            SettingsField::KbDeleteAgent => Some("Delete Agent"),
            SettingsField::KbAttach => Some("Attach to Agent"),
            SettingsField::KbSetNote => Some("Set Note"),
            SettingsField::KbYank => Some("Copy Name"),
            SettingsField::KbPause => Some("Pause Agent"),
            SettingsField::KbResume => Some("Resume/Refresh"),
            SettingsField::KbMerge => Some("Merge Main"),
            SettingsField::KbPush => Some("Push Changes"),
            SettingsField::KbFetch => Some("Fetch Remote"),
            SettingsField::KbSummary => Some("Request Summary"),
            SettingsField::KbToggleDiff => Some("Toggle Diff"),
            SettingsField::KbToggleLogs => Some("Toggle Logs"),
            SettingsField::KbOpenMr => Some("Open MR/PR"),
            SettingsField::KbAsanaAssign => Some("Assign Asana"),
            SettingsField::KbAsanaOpen => Some("Open in Asana"),
            SettingsField::KbRefreshAll => Some("Refresh All"),
            SettingsField::KbToggleHelp => Some("Toggle Help"),
            SettingsField::KbToggleSettings => Some("Toggle Settings"),
            SettingsField::KbQuit => Some("Quit"),
            SettingsField::KbOpenEditor => Some("Open in Editor"),
            SettingsField::KbShowTasks => Some("Show Tasks"),
            SettingsField::KbRefreshTaskList => Some("Refresh Task List"),
            _ => None,
        }
    }
}

impl SettingsItem {
    pub fn all_for_tab(
        tab: SettingsTab,
        provider: GitProvider,
        pm_provider: ProjectMgmtProvider,
    ) -> Vec<SettingsItem> {
        match tab {
            SettingsTab::General => vec![
                SettingsItem::Category(SettingsCategory::Agent),
                SettingsItem::Field(SettingsField::AiAgent),
                SettingsItem::Field(SettingsField::Editor),
                SettingsItem::Field(SettingsField::LogLevel),
                SettingsItem::Category(SettingsCategory::Storage),
                SettingsItem::Field(SettingsField::WorktreeLocation),
                SettingsItem::Category(SettingsCategory::Prompts),
                SettingsItem::Field(SettingsField::SummaryPrompt),
                SettingsItem::Field(SettingsField::MergePrompt),
                SettingsItem::Field(SettingsField::PushPrompt),
                SettingsItem::Category(SettingsCategory::Display),
                SettingsItem::Field(SettingsField::ShowPreview),
                SettingsItem::Field(SettingsField::ShowMetrics),
                SettingsItem::Field(SettingsField::ShowLogs),
                SettingsItem::Field(SettingsField::ShowBanner),
                SettingsItem::ActionButton(ActionButtonType::ResetTab),
                SettingsItem::ActionButton(ActionButtonType::ResetAll),
            ],
            SettingsTab::Git => {
                let mut items = vec![
                    SettingsItem::Category(SettingsCategory::GitProvider),
                    SettingsItem::Field(SettingsField::GitProvider),
                ];
                match provider {
                    GitProvider::GitLab => {
                        items.push(SettingsItem::Field(SettingsField::GitLabProjectId));
                        items.push(SettingsItem::Field(SettingsField::GitLabBaseUrl));
                    }
                    GitProvider::GitHub => {
                        items.push(SettingsItem::Field(SettingsField::GitHubOwner));
                        items.push(SettingsItem::Field(SettingsField::GitHubRepo));
                    }
                    GitProvider::Codeberg => {
                        items.push(SettingsItem::Field(SettingsField::CodebergOwner));
                        items.push(SettingsItem::Field(SettingsField::CodebergRepo));
                        items.push(SettingsItem::Field(SettingsField::CodebergBaseUrl));
                        items.push(SettingsItem::Category(SettingsCategory::Ci));
                        items.push(SettingsItem::Field(SettingsField::CodebergCiProvider));
                    }
                }
                items.push(SettingsItem::Category(SettingsCategory::GitConfig));
                items.push(SettingsItem::Field(SettingsField::BranchPrefix));
                items.push(SettingsItem::Field(SettingsField::MainBranch));
                items.push(SettingsItem::ActionButton(ActionButtonType::ResetTab));
                items
            }
            SettingsTab::ProjectMgmt => {
                let mut items = vec![
                    SettingsItem::Category(SettingsCategory::ProjectMgmt),
                    SettingsItem::Field(SettingsField::ProjectMgmtProvider),
                ];
                match pm_provider {
                    ProjectMgmtProvider::Asana => {
                        items.push(SettingsItem::Category(SettingsCategory::Asana));
                        items.push(SettingsItem::Field(SettingsField::AsanaProjectGid));
                        items.push(SettingsItem::Field(SettingsField::AsanaInProgressGid));
                        items.push(SettingsItem::Field(SettingsField::AsanaDoneGid));
                    }
                    ProjectMgmtProvider::Notion => {
                        items.push(SettingsItem::Category(SettingsCategory::Notion));
                        items.push(SettingsItem::Field(SettingsField::NotionDatabaseId));
                        items.push(SettingsItem::Field(SettingsField::NotionStatusProperty));
                        items.push(SettingsItem::Field(SettingsField::NotionInProgressOption));
                        items.push(SettingsItem::Field(SettingsField::NotionDoneOption));
                    }
                    ProjectMgmtProvider::Clickup => {
                        items.push(SettingsItem::Category(SettingsCategory::Clickup));
                        items.push(SettingsItem::Field(SettingsField::ClickUpListId));
                        items.push(SettingsItem::Field(SettingsField::ClickUpInProgressStatus));
                        items.push(SettingsItem::Field(SettingsField::ClickUpDoneStatus));
                    }
                }
                items.push(SettingsItem::ActionButton(ActionButtonType::ResetTab));
                items
            }
            SettingsTab::DevServer => vec![
                SettingsItem::Category(SettingsCategory::DevServer),
                SettingsItem::Field(SettingsField::DevServerCommand),
                SettingsItem::Field(SettingsField::DevServerRunBefore),
                SettingsItem::Field(SettingsField::DevServerWorkingDir),
                SettingsItem::Field(SettingsField::DevServerPort),
                SettingsItem::Field(SettingsField::DevServerAutoStart),
                SettingsItem::Field(SettingsField::WorktreeSymlinks),
                SettingsItem::ActionButton(ActionButtonType::ResetTab),
            ],
            SettingsTab::Keybinds => vec![
                SettingsItem::Category(SettingsCategory::KeybindNav),
                SettingsItem::Field(SettingsField::KbNavDown),
                SettingsItem::Field(SettingsField::KbNavUp),
                SettingsItem::Field(SettingsField::KbNavFirst),
                SettingsItem::Field(SettingsField::KbNavLast),
                SettingsItem::Category(SettingsCategory::KeybindAgent),
                SettingsItem::Field(SettingsField::KbNewAgent),
                SettingsItem::Field(SettingsField::KbDeleteAgent),
                SettingsItem::Field(SettingsField::KbAttach),
                SettingsItem::Field(SettingsField::KbSetNote),
                SettingsItem::Field(SettingsField::KbYank),
                SettingsItem::Category(SettingsCategory::KeybindGit),
                SettingsItem::Field(SettingsField::KbPause),
                SettingsItem::Field(SettingsField::KbResume),
                SettingsItem::Field(SettingsField::KbMerge),
                SettingsItem::Field(SettingsField::KbPush),
                SettingsItem::Field(SettingsField::KbFetch),
                SettingsItem::Field(SettingsField::KbSummary),
                SettingsItem::Field(SettingsField::KbToggleDiff),
                SettingsItem::Field(SettingsField::KbToggleLogs),
                SettingsItem::Category(SettingsCategory::KeybindExternal),
                SettingsItem::Field(SettingsField::KbOpenMr),
                SettingsItem::Field(SettingsField::KbAsanaAssign),
                SettingsItem::Field(SettingsField::KbAsanaOpen),
                SettingsItem::Field(SettingsField::KbOpenEditor),
                SettingsItem::Field(SettingsField::KbShowTasks),
                SettingsItem::Field(SettingsField::KbRefreshTaskList),
                SettingsItem::Category(SettingsCategory::KeybindOther),
                SettingsItem::Field(SettingsField::KbRefreshAll),
                SettingsItem::Field(SettingsField::KbToggleHelp),
                SettingsItem::Field(SettingsField::KbToggleSettings),
                SettingsItem::Field(SettingsField::KbQuit),
                SettingsItem::ActionButton(ActionButtonType::ResetTab),
            ],
        }
    }

    pub fn navigable_items(items: &[SettingsItem]) -> Vec<(usize, SettingsItem)> {
        items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| match item {
                SettingsItem::Field(f) => Some((i, SettingsItem::Field(*f))),
                SettingsItem::ActionButton(b) => Some((i, SettingsItem::ActionButton(*b))),
                SettingsItem::Category(_) => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub enum DropdownState {
    Closed,
    Open { selected_index: usize },
}

#[derive(Debug, Clone)]
pub struct FileBrowserState {
    pub active: bool,
    pub current_path: PathBuf,
    pub entries: Vec<DirEntry>,
    pub selected_index: usize,
    pub selected_files: HashSet<PathBuf>,
}

impl Default for FileBrowserState {
    fn default() -> Self {
        Self {
            active: false,
            current_path: PathBuf::new(),
            entries: Vec::new(),
            selected_index: 0,
            selected_files: HashSet::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SettingsState {
    pub active: bool,
    pub tab: SettingsTab,
    pub field_index: usize,
    pub dropdown: DropdownState,
    pub editing_text: bool,
    pub editing_prompt: bool,
    pub text_buffer: String,
    pub prompt_scroll: usize,
    pub pending_ai_agent: AiAgent,
    pub pending_editor: String,
    pub pending_log_level: ConfigLogLevel,
    pub pending_worktree_location: WorktreeLocation,
    pub pending_ui: UiConfig,
    pub repo_config: RepoConfig,
    pub pending_keybinds: Keybinds,
    pub capturing_keybind: Option<SettingsField>,
    pub keybind_conflicts: Vec<(String, String)>,
    pub file_browser: FileBrowserState,
    pub reset_confirmation: Option<ResetType>,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            active: false,
            tab: SettingsTab::General,
            field_index: 0,
            dropdown: DropdownState::Closed,
            editing_text: false,
            editing_prompt: false,
            text_buffer: String::new(),
            prompt_scroll: 0,
            pending_ai_agent: AiAgent::default(),
            pending_editor: String::new(),
            pending_log_level: ConfigLogLevel::default(),
            pending_worktree_location: WorktreeLocation::default(),
            pending_ui: UiConfig::default(),
            repo_config: RepoConfig::default(),
            pending_keybinds: Keybinds::default(),
            capturing_keybind: None,
            keybind_conflicts: Vec::new(),
            file_browser: FileBrowserState::default(),
            reset_confirmation: None,
        }
    }
}

impl SettingsState {
    pub fn all_items(&self) -> Vec<SettingsItem> {
        SettingsItem::all_for_tab(
            self.tab,
            self.repo_config.git.provider,
            self.repo_config.project_mgmt.provider,
        )
    }

    pub fn navigable_items(&self) -> Vec<(usize, SettingsItem)> {
        SettingsItem::navigable_items(&self.all_items())
    }

    pub fn current_item(&self) -> SettingsItem {
        let navigable = self.navigable_items();
        navigable
            .get(self.field_index)
            .map(|(_, item)| *item)
            .unwrap_or(SettingsItem::Field(SettingsField::AiAgent))
    }

    pub fn current_field(&self) -> SettingsField {
        match self.current_item() {
            SettingsItem::Field(f) => f,
            _ => SettingsField::AiAgent,
        }
    }

    pub fn current_action_button(&self) -> Option<ActionButtonType> {
        match self.current_item() {
            SettingsItem::ActionButton(btn) => Some(btn),
            _ => None,
        }
    }

    pub fn is_dropdown_open(&self) -> bool {
        matches!(self.dropdown, DropdownState::Open { .. })
    }

    pub fn total_fields(&self) -> usize {
        self.navigable_items().len()
    }

    pub fn next_tab(&self) -> SettingsTab {
        self.tab.next()
    }

    pub fn prev_tab(&self) -> SettingsTab {
        self.tab.prev()
    }

    pub fn get_keybind(&self, field: SettingsField) -> Option<&Keybind> {
        match field {
            SettingsField::KbNavDown => Some(&self.pending_keybinds.nav_down),
            SettingsField::KbNavUp => Some(&self.pending_keybinds.nav_up),
            SettingsField::KbNavFirst => Some(&self.pending_keybinds.nav_first),
            SettingsField::KbNavLast => Some(&self.pending_keybinds.nav_last),
            SettingsField::KbNewAgent => Some(&self.pending_keybinds.new_agent),
            SettingsField::KbDeleteAgent => Some(&self.pending_keybinds.delete_agent),
            SettingsField::KbAttach => Some(&self.pending_keybinds.attach),
            SettingsField::KbSetNote => Some(&self.pending_keybinds.set_note),
            SettingsField::KbYank => Some(&self.pending_keybinds.yank),
            SettingsField::KbPause => Some(&self.pending_keybinds.pause),
            SettingsField::KbResume => Some(&self.pending_keybinds.resume),
            SettingsField::KbMerge => Some(&self.pending_keybinds.merge),
            SettingsField::KbPush => Some(&self.pending_keybinds.push),
            SettingsField::KbFetch => Some(&self.pending_keybinds.fetch),
            SettingsField::KbSummary => Some(&self.pending_keybinds.summary),
            SettingsField::KbToggleDiff => Some(&self.pending_keybinds.toggle_diff),
            SettingsField::KbToggleLogs => Some(&self.pending_keybinds.toggle_logs),
            SettingsField::KbOpenMr => Some(&self.pending_keybinds.open_mr),
            SettingsField::KbAsanaAssign => Some(&self.pending_keybinds.asana_assign),
            SettingsField::KbAsanaOpen => Some(&self.pending_keybinds.asana_open),
            SettingsField::KbRefreshAll => Some(&self.pending_keybinds.refresh_all),
            SettingsField::KbToggleHelp => Some(&self.pending_keybinds.toggle_help),
            SettingsField::KbToggleSettings => Some(&self.pending_keybinds.toggle_settings),
            SettingsField::KbQuit => Some(&self.pending_keybinds.quit),
            SettingsField::KbOpenEditor => Some(&self.pending_keybinds.open_editor),
            SettingsField::KbShowTasks => Some(&self.pending_keybinds.show_tasks),
            SettingsField::KbRefreshTaskList => Some(&self.pending_keybinds.refresh_task_list),
            _ => None,
        }
    }

    pub fn set_keybind(&mut self, field: SettingsField, keybind: Keybind) {
        match field {
            SettingsField::KbNavDown => self.pending_keybinds.nav_down = keybind,
            SettingsField::KbNavUp => self.pending_keybinds.nav_up = keybind,
            SettingsField::KbNavFirst => self.pending_keybinds.nav_first = keybind,
            SettingsField::KbNavLast => self.pending_keybinds.nav_last = keybind,
            SettingsField::KbNewAgent => self.pending_keybinds.new_agent = keybind,
            SettingsField::KbDeleteAgent => self.pending_keybinds.delete_agent = keybind,
            SettingsField::KbAttach => self.pending_keybinds.attach = keybind,
            SettingsField::KbSetNote => self.pending_keybinds.set_note = keybind,
            SettingsField::KbYank => self.pending_keybinds.yank = keybind,
            SettingsField::KbPause => self.pending_keybinds.pause = keybind,
            SettingsField::KbResume => self.pending_keybinds.resume = keybind,
            SettingsField::KbMerge => self.pending_keybinds.merge = keybind,
            SettingsField::KbPush => self.pending_keybinds.push = keybind,
            SettingsField::KbFetch => self.pending_keybinds.fetch = keybind,
            SettingsField::KbSummary => self.pending_keybinds.summary = keybind,
            SettingsField::KbToggleDiff => self.pending_keybinds.toggle_diff = keybind,
            SettingsField::KbToggleLogs => self.pending_keybinds.toggle_logs = keybind,
            SettingsField::KbOpenMr => self.pending_keybinds.open_mr = keybind,
            SettingsField::KbAsanaAssign => self.pending_keybinds.asana_assign = keybind,
            SettingsField::KbAsanaOpen => self.pending_keybinds.asana_open = keybind,
            SettingsField::KbRefreshAll => self.pending_keybinds.refresh_all = keybind,
            SettingsField::KbToggleHelp => self.pending_keybinds.toggle_help = keybind,
            SettingsField::KbToggleSettings => self.pending_keybinds.toggle_settings = keybind,
            SettingsField::KbQuit => self.pending_keybinds.quit = keybind,
            SettingsField::KbOpenEditor => self.pending_keybinds.open_editor = keybind,
            SettingsField::KbShowTasks => self.pending_keybinds.show_tasks = keybind,
            SettingsField::KbRefreshTaskList => self.pending_keybinds.refresh_task_list = keybind,
            _ => {}
        }
        self.keybind_conflicts = self.pending_keybinds.find_conflicts();
    }

    pub fn has_keybind_conflicts(&self) -> bool {
        !self.keybind_conflicts.is_empty()
    }

    pub fn init_file_browser(&mut self, repo_path: &str) {
        let repo_path = PathBuf::from(repo_path);
        let symlinks = &self.repo_config.dev_server.worktree_symlinks;

        let mut selected_files = HashSet::new();
        for symlink in symlinks {
            selected_files.insert(repo_path.join(symlink));
        }

        let entries = crate::ui::components::file_browser::load_directory_entries(
            &repo_path,
            &selected_files,
            &repo_path,
        );

        self.file_browser = FileBrowserState {
            active: true,
            current_path: repo_path,
            entries,
            selected_index: 0,
            selected_files,
        };
    }

    pub fn is_file_browser_active(&self) -> bool {
        self.file_browser.active
    }

    pub fn reset_general_defaults(&mut self) {
        self.pending_ai_agent = AiAgent::default();
        self.pending_editor = "code {path}".to_string();
        self.pending_log_level = ConfigLogLevel::default();
        self.pending_worktree_location = WorktreeLocation::default();
        self.pending_ui = UiConfig::default();
        self.repo_config.prompts = crate::app::config::PromptsConfig::default();
    }

    pub fn reset_git_defaults(&mut self) {
        self.repo_config.git = crate::app::config::RepoGitConfig::default();
    }

    pub fn reset_project_mgmt_defaults(&mut self) {
        self.repo_config.project_mgmt = crate::app::config::RepoProjectMgmtConfig::default();
    }

    pub fn reset_dev_server_defaults(&mut self) {
        self.repo_config.dev_server = crate::app::config::DevServerConfig::default();
    }

    pub fn reset_keybinds_defaults(&mut self) {
        self.pending_keybinds = Keybinds::default();
        self.keybind_conflicts.clear();
    }

    pub fn reset_current_tab(&mut self) {
        match self.tab {
            SettingsTab::General => self.reset_general_defaults(),
            SettingsTab::Git => self.reset_git_defaults(),
            SettingsTab::ProjectMgmt => self.reset_project_mgmt_defaults(),
            SettingsTab::DevServer => self.reset_dev_server_defaults(),
            SettingsTab::Keybinds => self.reset_keybinds_defaults(),
        }
    }

    pub fn reset_all(&mut self) {
        self.reset_general_defaults();
        self.reset_git_defaults();
        self.reset_project_mgmt_defaults();
        self.reset_dev_server_defaults();
        self.reset_keybinds_defaults();
    }
}

#[derive(Debug)]
pub struct AppState {
    pub agents: HashMap<Uuid, Agent>,
    pub agent_order: Vec<Uuid>,
    pub selected_index: usize,
    pub config: Config,
    pub running: bool,
    pub toast: Option<Toast>,
    pub show_help: bool,
    pub show_diff: bool,
    pub input_mode: Option<InputMode>,
    pub input_buffer: String,
    pub output_scroll: usize,
    pub repo_path: String,
    pub logs: Vec<LogEntry>,
    pub show_logs: bool,
    pub animation_frame: usize,
    pub cpu_history: VecDeque<f32>,
    pub memory_history: VecDeque<f32>,
    pub memory_used: u64,
    pub memory_total: u64,
    pub loading_message: Option<String>,
    pub preview_content: Option<String>,
    pub settings: SettingsState,
    pub show_global_setup: bool,
    pub global_setup: Option<GlobalSetupState>,
    pub show_project_setup: bool,
    pub project_setup: Option<ProjectSetupState>,
    pub worktree_base: std::path::PathBuf,
    pub preview_tab: PreviewTab,
    pub devserver_scroll: usize,
    pub devserver_warning: Option<DevServerWarning>,
    pub task_reassignment_warning: Option<TaskReassignmentWarning>,
    pub task_list: Vec<TaskListItem>,
    pub task_list_loading: bool,
    pub task_list_selected: usize,
    pub task_list_expanded_ids: HashSet<String>,
    pub task_status_dropdown: Option<TaskStatusDropdownState>,
    pub subtask_status_dropdown: Option<SubtaskStatusDropdownState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlobalSetupStep {
    #[default]
    WorktreeLocation,
    AgentSettings,
}

#[derive(Debug, Clone, Default)]
pub struct GlobalSetupState {
    pub step: GlobalSetupStep,
    pub worktree_location: WorktreeLocation,
    pub ai_agent: AiAgent,
    pub log_level: ConfigLogLevel,
    pub field_index: usize,
    pub dropdown_open: bool,
    pub dropdown_index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectSetupState {
    pub config: RepoConfig,
    pub field_index: usize,
    pub dropdown_open: bool,
    pub dropdown_index: usize,
    pub editing_text: bool,
    pub text_buffer: String,
}

#[derive(Debug, Clone)]
pub struct TaskStatusDropdownState {
    pub agent_id: Uuid,
    pub status_options: Vec<StatusOption>,
    pub selected_index: usize,
}

#[derive(Debug, Clone)]
pub struct SubtaskStatusDropdownState {
    pub task_id: String,
    pub task_name: String,
    pub current_completed: bool,
    pub selected_index: usize,
}

#[derive(Debug, Clone)]
pub struct StatusOption {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Success,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub level: ToastLevel,
    pub created_at: std::time::Instant,
    pub duration_secs: u64,
}

impl Toast {
    pub fn new(message: String, level: ToastLevel) -> Self {
        let duration_secs = match level {
            ToastLevel::Success => 3,
            ToastLevel::Info => 3,
            ToastLevel::Warning => 4,
            ToastLevel::Error => 5,
        };
        Self {
            message,
            level,
            created_at: std::time::Instant::now(),
            duration_secs,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed().as_secs() >= self.duration_secs
    }
}

impl AppState {
    pub fn new(config: Config, repo_path: String) -> Self {
        let repo_config = RepoConfig::load(&repo_path).unwrap_or_default();
        let show_logs = config.ui.show_logs;
        let pending_keybinds = config.keybinds.clone();

        let worktree_base = config.worktree_base_path(&repo_path);

        Self {
            agents: HashMap::new(),
            agent_order: Vec::new(),
            selected_index: 0,
            config,
            running: true,
            toast: None,
            show_help: false,
            show_diff: false,
            input_mode: None,
            input_buffer: String::new(),
            output_scroll: 0,
            repo_path,
            logs: Vec::new(),
            show_logs,
            animation_frame: 0,
            cpu_history: VecDeque::with_capacity(SYSTEM_METRICS_HISTORY_SIZE),
            memory_history: VecDeque::with_capacity(SYSTEM_METRICS_HISTORY_SIZE),
            memory_used: 0,
            memory_total: 0,
            loading_message: None,
            preview_content: None,
            settings: SettingsState {
                pending_ai_agent: AiAgent::default(),
                pending_log_level: ConfigLogLevel::default(),
                pending_worktree_location: WorktreeLocation::default(),
                pending_keybinds,
                repo_config,
                ..Default::default()
            },
            show_global_setup: false,
            global_setup: None,
            show_project_setup: false,
            project_setup: None,
            worktree_base,
            preview_tab: PreviewTab::default(),
            devserver_scroll: 0,
            devserver_warning: None,
            task_reassignment_warning: None,
            task_list: Vec::new(),
            task_list_loading: false,
            task_list_selected: 0,
            task_list_expanded_ids: HashSet::new(),
            task_status_dropdown: None,
            subtask_status_dropdown: None,
        }
    }

    pub fn advance_animation(&mut self) {
        self.animation_frame = (self.animation_frame + 1) % 10;
    }

    pub fn log(&mut self, level: LogLevel, message: impl Into<String>) {
        let entry = LogEntry {
            timestamp: Utc::now(),
            level,
            message: message.into(),
        };
        self.logs.push(entry);
        if self.logs.len() > 100 {
            self.logs.remove(0);
        }
    }

    pub fn log_info(&mut self, message: impl Into<String>) {
        self.log(LogLevel::Info, message);
    }

    pub fn log_warn(&mut self, message: impl Into<String>) {
        self.log(LogLevel::Warn, message);
    }

    pub fn log_error(&mut self, message: impl Into<String>) {
        self.log(LogLevel::Error, message);
    }

    pub fn log_debug(&mut self, message: impl Into<String>) {
        self.log(LogLevel::Debug, message);
    }

    pub fn selected_agent(&self) -> Option<&Agent> {
        self.agent_order
            .get(self.selected_index)
            .and_then(|id| self.agents.get(id))
    }

    pub fn selected_agent_mut(&mut self) -> Option<&mut Agent> {
        self.agent_order
            .get(self.selected_index)
            .cloned()
            .and_then(move |id| self.agents.get_mut(&id))
    }

    pub fn selected_agent_id(&self) -> Option<Uuid> {
        self.agent_order.get(self.selected_index).cloned()
    }

    pub fn add_agent(&mut self, agent: Agent) {
        let id = agent.id;
        self.agents.insert(id, agent);
        self.agent_order.push(id);
        self.sort_agents_by_created();
    }

    fn sort_agents_by_created(&mut self) {
        let agents = &self.agents;
        self.agent_order.sort_by(|a, b| {
            let a_time = agents.get(a).map(|a| a.created_at);
            let b_time = agents.get(b).map(|b| b.created_at);
            a_time.cmp(&b_time)
        });
    }

    pub fn remove_agent(&mut self, id: Uuid) -> Option<Agent> {
        if let Some(pos) = self.agent_order.iter().position(|&x| x == id) {
            self.agent_order.remove(pos);
            if self.selected_index >= self.agent_order.len() && self.selected_index > 0 {
                self.selected_index -= 1;
            }
        }
        self.agents.remove(&id)
    }

    pub fn select_next(&mut self) {
        if !self.agent_order.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.agent_order.len();
            self.output_scroll = 0;
        }
    }

    pub fn select_previous(&mut self) {
        if !self.agent_order.is_empty() {
            self.selected_index = if self.selected_index == 0 {
                self.agent_order.len() - 1
            } else {
                self.selected_index - 1
            };
            self.output_scroll = 0;
        }
    }

    pub fn select_first(&mut self) {
        self.selected_index = 0;
        self.output_scroll = 0;
    }

    pub fn select_last(&mut self) {
        if !self.agent_order.is_empty() {
            self.selected_index = self.agent_order.len() - 1;
            self.output_scroll = 0;
        }
    }

    pub fn is_input_mode(&self) -> bool {
        self.input_mode.is_some()
    }

    pub fn enter_input_mode(&mut self, mode: InputMode) {
        self.input_mode = Some(mode);
        self.input_buffer.clear();
    }

    pub fn exit_input_mode(&mut self) {
        self.input_mode = None;
        self.input_buffer.clear();
        self.task_status_dropdown = None;
        self.subtask_status_dropdown = None;
    }

    pub fn show_error(&mut self, msg: impl Into<String>) {
        self.toast = Some(Toast::new(msg.into(), ToastLevel::Error));
    }

    pub fn show_success(&mut self, msg: impl Into<String>) {
        self.toast = Some(Toast::new(msg.into(), ToastLevel::Success));
    }

    pub fn show_info(&mut self, msg: impl Into<String>) {
        self.toast = Some(Toast::new(msg.into(), ToastLevel::Info));
    }

    pub fn show_warning(&mut self, msg: impl Into<String>) {
        self.toast = Some(Toast::new(msg.into(), ToastLevel::Warning));
    }

    pub fn record_system_metrics(&mut self, cpu_percent: f32, memory_used: u64, memory_total: u64) {
        if self.cpu_history.len() >= SYSTEM_METRICS_HISTORY_SIZE {
            self.cpu_history.pop_front();
        }
        self.cpu_history.push_back(cpu_percent);

        let memory_percent = if memory_total > 0 {
            (memory_used as f64 / memory_total as f64 * 100.0) as f32
        } else {
            0.0
        };
        if self.memory_history.len() >= SYSTEM_METRICS_HISTORY_SIZE {
            self.memory_history.pop_front();
        }
        self.memory_history.push_back(memory_percent);

        self.memory_used = memory_used;
        self.memory_total = memory_total;
    }
}
