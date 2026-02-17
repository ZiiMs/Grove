use chrono::Utc;
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

use super::action::InputMode;
use super::config::Config;
use crate::agent::Agent;

/// Maximum number of system metrics samples to keep for the graphs
const SYSTEM_METRICS_HISTORY_SIZE: usize = 60;

/// The single source of truth for application state.
#[derive(Debug)]
pub struct AppState {
    /// All managed agents, keyed by ID
    pub agents: HashMap<Uuid, Agent>,
    /// Ordered list of agent IDs for display
    pub agent_order: Vec<Uuid>,
    /// Currently selected agent index
    pub selected_index: usize,
    /// Application configuration
    pub config: Config,
    /// Whether the app is running
    pub running: bool,
    /// Current error message to display
    pub error_message: Option<String>,
    /// Whether help overlay is shown
    pub show_help: bool,
    /// Whether diff view is active
    pub show_diff: bool,
    /// Current input mode (for text input)
    pub input_mode: Option<InputMode>,
    /// Current input buffer
    pub input_buffer: String,
    /// Scroll offset for output view
    pub output_scroll: usize,
    /// Path to the main repository
    pub repo_path: String,
    /// Log messages for debugging
    pub logs: Vec<LogEntry>,
    /// Whether to show log panel
    pub show_logs: bool,
    /// Animation frame counter (0-9, cycles for spinner)
    pub animation_frame: usize,
    /// Global CPU usage history (percentage 0-100)
    pub cpu_history: VecDeque<f32>,
    /// Global memory usage history (percentage 0-100)
    pub memory_history: VecDeque<f32>,
    /// Current memory used in bytes
    pub memory_used: u64,
    /// Total memory in bytes
    pub memory_total: u64,
    /// Loading message (shown as overlay when Some)
    pub loading_message: Option<String>,
    /// Preview content for the selected agent (with ANSI codes)
    pub preview_content: Option<String>,
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
            show_logs: true, // Show logs by default for debugging
            animation_frame: 0,
            cpu_history: VecDeque::with_capacity(SYSTEM_METRICS_HISTORY_SIZE),
            memory_history: VecDeque::with_capacity(SYSTEM_METRICS_HISTORY_SIZE),
            memory_used: 0,
            memory_total: 0,
            loading_message: None,
            preview_content: None,
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
