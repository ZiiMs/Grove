use chrono::Utc;
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

use super::action::InputMode;
use super::config::{AiAgent, Config, GitProvider, LogLevel as ConfigLogLevel, ProjectConfig};
use crate::agent::Agent;

const SYSTEM_METRICS_HISTORY_SIZE: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Global,
    Project,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    AiAgent,
    GitProvider,
    LogLevel,
    BranchPrefix,
    MainBranch,
}

impl SettingsField {
    pub fn section(&self) -> SettingsSection {
        match self {
            SettingsField::AiAgent | SettingsField::GitProvider | SettingsField::LogLevel => {
                SettingsSection::Global
            }
            SettingsField::BranchPrefix | SettingsField::MainBranch => SettingsSection::Project,
        }
    }

    pub fn all_for_section(section: SettingsSection) -> &'static [SettingsField] {
        match section {
            SettingsSection::Global => &[
                SettingsField::AiAgent,
                SettingsField::GitProvider,
                SettingsField::LogLevel,
            ],
            SettingsSection::Project => &[SettingsField::BranchPrefix, SettingsField::MainBranch],
        }
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
    pub section: SettingsSection,
    pub field_index: usize,
    pub dropdown: DropdownState,
    pub editing_text: bool,
    pub text_buffer: String,
    pub pending_ai_agent: AiAgent,
    pub pending_git_provider: GitProvider,
    pub pending_log_level: ConfigLogLevel,
    pub project_config: ProjectConfig,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            active: false,
            section: SettingsSection::Global,
            field_index: 0,
            dropdown: DropdownState::Closed,
            editing_text: false,
            text_buffer: String::new(),
            pending_ai_agent: AiAgent::default(),
            pending_git_provider: GitProvider::default(),
            pending_log_level: ConfigLogLevel::default(),
            project_config: ProjectConfig::default(),
        }
    }
}

impl SettingsState {
    pub fn current_field(&self) -> SettingsField {
        SettingsField::all_for_section(self.section)
            .get(self.field_index)
            .copied()
            .unwrap_or(SettingsField::AiAgent)
    }

    pub fn is_dropdown_open(&self) -> bool {
        matches!(self.dropdown, DropdownState::Open { .. })
    }

    pub fn total_fields(&self) -> usize {
        SettingsField::all_for_section(self.section).len()
    }
}

/// The single source of truth for application state.
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
}

/// A log entry with timestamp and level
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
        let project_config = ProjectConfig::load(&repo_path).unwrap_or_default();
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
            show_logs: true,
            animation_frame: 0,
            cpu_history: VecDeque::with_capacity(SYSTEM_METRICS_HISTORY_SIZE),
            memory_history: VecDeque::with_capacity(SYSTEM_METRICS_HISTORY_SIZE),
            memory_used: 0,
            memory_total: 0,
            loading_message: None,
            preview_content: None,
            settings: SettingsState {
                pending_ai_agent: AiAgent::default(),
                pending_git_provider: GitProvider::default(),
                pending_log_level: ConfigLogLevel::default(),
                project_config,
                ..Default::default()
            },
        }
    }

    /// Advance the animation frame (cycles 0-9)
    pub fn advance_animation(&mut self) {
        self.animation_frame = (self.animation_frame + 1) % 10;
    }

    /// Add a log entry
    pub fn log(&mut self, level: LogLevel, message: impl Into<String>) {
        let entry = LogEntry {
            timestamp: Utc::now(),
            level,
            message: message.into(),
        };
        self.logs.push(entry);
        // Keep only last 100 logs
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

    /// Get the currently selected agent
    pub fn selected_agent(&self) -> Option<&Agent> {
        self.agent_order
            .get(self.selected_index)
            .and_then(|id| self.agents.get(id))
    }

    /// Get the currently selected agent mutably
    pub fn selected_agent_mut(&mut self) -> Option<&mut Agent> {
        self.agent_order
            .get(self.selected_index)
            .cloned()
            .and_then(move |id| self.agents.get_mut(&id))
    }

    /// Get the ID of the currently selected agent
    pub fn selected_agent_id(&self) -> Option<Uuid> {
        self.agent_order.get(self.selected_index).cloned()
    }

    /// Add a new agent
    pub fn add_agent(&mut self, agent: Agent) {
        let id = agent.id;
        self.agents.insert(id, agent);
        self.agent_order.push(id);
        self.sort_agents_by_created();
    }

    /// Sort agent_order by creation time (oldest first)
    fn sort_agents_by_created(&mut self) {
        let agents = &self.agents;
        self.agent_order.sort_by(|a, b| {
            let a_time = agents.get(a).map(|a| a.created_at);
            let b_time = agents.get(b).map(|b| b.created_at);
            a_time.cmp(&b_time)
        });
    }

    /// Remove an agent by ID
    pub fn remove_agent(&mut self, id: Uuid) -> Option<Agent> {
        if let Some(pos) = self.agent_order.iter().position(|&x| x == id) {
            self.agent_order.remove(pos);
            // Adjust selected index if needed
            if self.selected_index >= self.agent_order.len() && self.selected_index > 0 {
                self.selected_index -= 1;
            }
        }
        self.agents.remove(&id)
    }

    /// Move selection to next agent
    pub fn select_next(&mut self) {
        if !self.agent_order.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.agent_order.len();
            self.output_scroll = 0;
        }
    }

    /// Move selection to previous agent
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

    /// Select first agent
    pub fn select_first(&mut self) {
        self.selected_index = 0;
        self.output_scroll = 0;
    }

    /// Select last agent
    pub fn select_last(&mut self) {
        if !self.agent_order.is_empty() {
            self.selected_index = self.agent_order.len() - 1;
            self.output_scroll = 0;
        }
    }

    /// Check if we're in input mode
    pub fn is_input_mode(&self) -> bool {
        self.input_mode.is_some()
    }

    /// Enter input mode
    pub fn enter_input_mode(&mut self, mode: InputMode) {
        self.input_mode = Some(mode);
        self.input_buffer.clear();
    }

    /// Exit input mode
    pub fn exit_input_mode(&mut self) {
        self.input_mode = None;
        self.input_buffer.clear();
    }

    /// Record global system metrics
    pub fn record_system_metrics(&mut self, cpu_percent: f32, memory_used: u64, memory_total: u64) {
        // CPU history
        if self.cpu_history.len() >= SYSTEM_METRICS_HISTORY_SIZE {
            self.cpu_history.pop_front();
        }
        self.cpu_history.push_back(cpu_percent);

        // Memory history (as percentage)
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
