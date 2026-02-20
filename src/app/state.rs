use chrono::Utc;
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

use super::action::InputMode;
use super::config::{
    AiAgent, Config, GitProvider, LogLevel as ConfigLogLevel, ProjectMgmtProvider, RepoConfig,
    UiConfig, WorktreeLocation,
};
use super::task_list::TaskListItem;
use crate::agent::Agent;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Git,
    ProjectMgmt,
    DevServer,
}

impl SettingsTab {
    pub fn all() -> &'static [SettingsTab] {
        &[
            SettingsTab::General,
            SettingsTab::Git,
            SettingsTab::ProjectMgmt,
            SettingsTab::DevServer,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            SettingsTab::General => "General",
            SettingsTab::Git => "Git",
            SettingsTab::ProjectMgmt => "Project Mgmt",
            SettingsTab::DevServer => "Dev Server",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            SettingsTab::General => SettingsTab::Git,
            SettingsTab::Git => SettingsTab::ProjectMgmt,
            SettingsTab::ProjectMgmt => SettingsTab::DevServer,
            SettingsTab::DevServer => SettingsTab::General,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            SettingsTab::General => SettingsTab::DevServer,
            SettingsTab::Git => SettingsTab::General,
            SettingsTab::ProjectMgmt => SettingsTab::Git,
            SettingsTab::DevServer => SettingsTab::ProjectMgmt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    AiAgent,
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
    SummaryPrompt,
    MergePrompt,
    PushPrompt,
    DevServerCommand,
    DevServerRunBefore,
    DevServerWorkingDir,
    DevServerPort,
    DevServerAutoStart,
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
    Prompts,
    DevServer,
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
            SettingsCategory::Prompts => "Prompts",
            SettingsCategory::DevServer => "Dev Server",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsItem {
    Category(SettingsCategory),
    Field(SettingsField),
}

impl SettingsField {
    pub fn tab(&self) -> SettingsTab {
        match self {
            SettingsField::AiAgent
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
            | SettingsField::MainBranch
            | SettingsField::WorktreeSymlinks => SettingsTab::Git,
            SettingsField::ProjectMgmtProvider
            | SettingsField::AsanaProjectGid
            | SettingsField::AsanaInProgressGid
            | SettingsField::AsanaDoneGid
            | SettingsField::NotionDatabaseId
            | SettingsField::NotionStatusProperty
            | SettingsField::NotionInProgressOption
            | SettingsField::NotionDoneOption => SettingsTab::ProjectMgmt,
            SettingsField::DevServerCommand
            | SettingsField::DevServerRunBefore
            | SettingsField::DevServerWorkingDir
            | SettingsField::DevServerPort
            | SettingsField::DevServerAutoStart => SettingsTab::DevServer,
        }
    }

    pub fn is_prompt_field(&self) -> bool {
        matches!(
            self,
            SettingsField::SummaryPrompt | SettingsField::MergePrompt | SettingsField::PushPrompt
        )
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
                items.push(SettingsItem::Field(SettingsField::WorktreeSymlinks));
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
                }
                items
            }
            SettingsTab::DevServer => vec![
                SettingsItem::Category(SettingsCategory::DevServer),
                SettingsItem::Field(SettingsField::DevServerCommand),
                SettingsItem::Field(SettingsField::DevServerRunBefore),
                SettingsItem::Field(SettingsField::DevServerWorkingDir),
                SettingsItem::Field(SettingsField::DevServerPort),
                SettingsItem::Field(SettingsField::DevServerAutoStart),
            ],
        }
    }

    pub fn navigable_items(items: &[SettingsItem]) -> Vec<(usize, SettingsField)> {
        items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| match item {
                SettingsItem::Field(f) => Some((i, *f)),
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
    pub pending_log_level: ConfigLogLevel,
    pub pending_worktree_location: WorktreeLocation,
    pub pending_ui: UiConfig,
    pub repo_config: RepoConfig,
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
            pending_log_level: ConfigLogLevel::default(),
            pending_worktree_location: WorktreeLocation::default(),
            pending_ui: UiConfig::default(),
            repo_config: RepoConfig::default(),
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

    pub fn navigable_items(&self) -> Vec<(usize, SettingsField)> {
        SettingsItem::navigable_items(&self.all_items())
    }

    pub fn current_field(&self) -> SettingsField {
        let navigable = self.navigable_items();
        navigable
            .get(self.field_index)
            .map(|(_, f)| *f)
            .unwrap_or(SettingsField::AiAgent)
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
}

#[derive(Debug)]
pub struct AppState {
    pub agents: HashMap<Uuid, Agent>,
    pub agent_order: Vec<Uuid>,
    pub selected_index: usize,
    pub config: Config,
    pub running: bool,
    pub error_message: Option<String>,
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
    pub task_list: Vec<TaskListItem>,
    pub task_list_loading: bool,
    pub task_list_selected: usize,
    pub task_status_dropdown: Option<TaskStatusDropdownState>,
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

impl AppState {
    pub fn new(config: Config, repo_path: String) -> Self {
        let repo_config = RepoConfig::load(&repo_path).unwrap_or_default();
        let show_logs = config.ui.show_logs;

        let worktree_base = config.worktree_base_path(&repo_path);

        Self {
            agents: HashMap::new(),
            agent_order: Vec::new(),
            selected_index: 0,
            config,
            running: true,
            error_message: None,
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
            task_list: Vec::new(),
            task_list_loading: false,
            task_list_selected: 0,
            task_status_dropdown: None,
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
