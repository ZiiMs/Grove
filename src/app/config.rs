use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AiAgent {
    #[default]
    ClaudeCode,
    Opencode,
    Codex,
    Gemini,
}

impl AiAgent {
    pub fn display_name(&self) -> &'static str {
        match self {
            AiAgent::ClaudeCode => "Claude Code",
            AiAgent::Opencode => "Opencode",
            AiAgent::Codex => "Codex",
            AiAgent::Gemini => "Gemini",
        }
    }

    pub fn all() -> &'static [AiAgent] {
        &[
            AiAgent::ClaudeCode,
            AiAgent::Opencode,
            AiAgent::Codex,
            AiAgent::Gemini,
        ]
    }

    pub fn command(&self) -> &'static str {
        match self {
            AiAgent::ClaudeCode => "claude",
            AiAgent::Opencode => "opencode",
            AiAgent::Codex => "codex",
            AiAgent::Gemini => "gemini",
        }
    }

    pub fn push_command(&self) -> Option<&'static str> {
        match self {
            AiAgent::ClaudeCode => Some("/push"),
            AiAgent::Opencode => None,
            AiAgent::Codex => None,
            AiAgent::Gemini => None,
        }
    }

    pub fn push_prompt(&self) -> Option<&'static str> {
        match self {
            AiAgent::ClaudeCode => None,
            AiAgent::Opencode => {
                Some("Review the changes, then commit and push them to the remote branch.")
            }
            AiAgent::Codex => Some("Please commit and push these changes"),
            AiAgent::Gemini => Some("Please commit and push these changes"),
        }
    }

    pub fn process_names(&self) -> &'static [&'static str] {
        match self {
            AiAgent::ClaudeCode => &["node", "claude", "npx"],
            AiAgent::Opencode => &["node", "opencode", "npx"],
            AiAgent::Codex => &["codex"],
            AiAgent::Gemini => &["node", "gemini"],
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum GitProvider {
    #[default]
    GitLab,
    GitHub,
    Codeberg,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CodebergCiProvider {
    #[default]
    ForgejoActions,
    Woodpecker,
}

impl CodebergCiProvider {
    pub fn display_name(&self) -> &'static str {
        match self {
            CodebergCiProvider::ForgejoActions => "Forgejo Actions",
            CodebergCiProvider::Woodpecker => "Woodpecker CI",
        }
    }

    pub fn all() -> &'static [CodebergCiProvider] {
        &[
            CodebergCiProvider::ForgejoActions,
            CodebergCiProvider::Woodpecker,
        ]
    }
}

impl GitProvider {
    pub fn display_name(&self) -> &'static str {
        match self {
            GitProvider::GitLab => "GitLab",
            GitProvider::GitHub => "GitHub",
            GitProvider::Codeberg => "Codeberg",
        }
    }

    pub fn all() -> &'static [GitProvider] {
        &[
            GitProvider::GitLab,
            GitProvider::GitHub,
            GitProvider::Codeberg,
        ]
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProjectMgmtProvider {
    #[default]
    Asana,
    Notion,
    Clickup,
    Airtable,
    Linear,
}

impl ProjectMgmtProvider {
    pub fn display_name(&self) -> &'static str {
        match self {
            ProjectMgmtProvider::Asana => "Asana",
            ProjectMgmtProvider::Notion => "Notion",
            ProjectMgmtProvider::Clickup => "ClickUp",
            ProjectMgmtProvider::Airtable => "Airtable",
            ProjectMgmtProvider::Linear => "Linear",
        }
    }

    pub fn all() -> &'static [ProjectMgmtProvider] {
        &[
            ProjectMgmtProvider::Asana,
            ProjectMgmtProvider::Notion,
            ProjectMgmtProvider::Clickup,
            ProjectMgmtProvider::Airtable,
            ProjectMgmtProvider::Linear,
        ]
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn display_name(&self) -> &'static str {
        match self {
            LogLevel::Debug => "Debug",
            LogLevel::Info => "Info",
            LogLevel::Warn => "Warn",
            LogLevel::Error => "Error",
        }
    }

    pub fn all() -> &'static [LogLevel] {
        &[
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warn,
            LogLevel::Error,
        ]
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WorktreeLocation {
    #[default]
    Project,
    Home,
}

impl WorktreeLocation {
    pub fn display_name(&self) -> &'static str {
        match self {
            WorktreeLocation::Project => "Project directory",
            WorktreeLocation::Home => "Home directory",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            WorktreeLocation::Project => ".worktrees/ alongside your repo",
            WorktreeLocation::Home => "~/.grove/worktrees/ (keeps repo clean)",
        }
    }

    pub fn all() -> &'static [WorktreeLocation] {
        &[WorktreeLocation::Project, WorktreeLocation::Home]
    }
}

fn default_editor() -> String {
    "code {path}".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalConfig {
    #[serde(default)]
    pub ai_agent: AiAgent,
    #[serde(default)]
    pub log_level: LogLevel,
    #[serde(default)]
    pub worktree_location: WorktreeLocation,
    #[serde(default = "default_editor")]
    pub editor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub global: GlobalConfig,
    #[serde(default)]
    pub gitlab: GitLabConfig,
    #[serde(default)]
    pub asana: AsanaConfig,
    #[serde(default)]
    pub notion: NotionConfig,
    #[serde(default)]
    pub clickup: ClickUpConfig,
    #[serde(default)]
    pub airtable: AirtableConfig,
    #[serde(default)]
    pub linear: LinearConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub performance: PerformanceConfig,
    #[serde(default)]
    pub keybinds: Keybinds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsanaConfig {
    #[serde(default = "default_asana_refresh")]
    pub refresh_secs: u64,
    #[serde(default = "default_asana_cache_ttl")]
    pub cache_ttl_secs: u64,
}

fn default_asana_refresh() -> u64 {
    120
}

fn default_asana_cache_ttl() -> u64 {
    60
}

impl Default for AsanaConfig {
    fn default() -> Self {
        Self {
            refresh_secs: default_asana_refresh(),
            cache_ttl_secs: default_asana_cache_ttl(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotionConfig {
    #[serde(default = "default_notion_refresh")]
    pub refresh_secs: u64,
    #[serde(default = "default_notion_cache_ttl")]
    pub cache_ttl_secs: u64,
}

fn default_notion_refresh() -> u64 {
    120
}

fn default_notion_cache_ttl() -> u64 {
    60
}

impl Default for NotionConfig {
    fn default() -> Self {
        Self {
            refresh_secs: default_notion_refresh(),
            cache_ttl_secs: default_notion_cache_ttl(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickUpConfig {
    #[serde(default = "default_clickup_refresh")]
    pub refresh_secs: u64,
    #[serde(default = "default_clickup_cache_ttl")]
    pub cache_ttl_secs: u64,
}

fn default_clickup_refresh() -> u64 {
    120
}

fn default_clickup_cache_ttl() -> u64 {
    60
}

impl Default for ClickUpConfig {
    fn default() -> Self {
        Self {
            refresh_secs: default_clickup_refresh(),
            cache_ttl_secs: default_clickup_cache_ttl(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirtableConfig {
    #[serde(default = "default_airtable_refresh")]
    pub refresh_secs: u64,
    #[serde(default = "default_airtable_cache_ttl")]
    pub cache_ttl_secs: u64,
}

fn default_airtable_refresh() -> u64 {
    120
}

fn default_airtable_cache_ttl() -> u64 {
    60
}

impl Default for AirtableConfig {
    fn default() -> Self {
        Self {
            refresh_secs: default_airtable_refresh(),
            cache_ttl_secs: default_airtable_cache_ttl(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearConfig {
    #[serde(default = "default_linear_refresh")]
    pub refresh_secs: u64,
    #[serde(default = "default_linear_cache_ttl")]
    pub cache_ttl_secs: u64,
}

fn default_linear_refresh() -> u64 {
    120
}

fn default_linear_cache_ttl() -> u64 {
    60
}

impl Default for LinearConfig {
    fn default() -> Self {
        Self {
            refresh_secs: default_linear_refresh(),
            cache_ttl_secs: default_linear_cache_ttl(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabConfig {
    #[serde(default = "default_gitlab_url")]
    pub base_url: String,
}

fn default_gitlab_url() -> String {
    "https://gitlab.com".to_string()
}

impl Default for GitLabConfig {
    fn default() -> Self {
        Self {
            base_url: default_gitlab_url(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_frame_rate")]
    pub frame_rate: u32,
    #[serde(default = "default_tick_rate")]
    pub tick_rate_ms: u64,
    #[serde(default = "default_output_buffer")]
    pub output_buffer_lines: usize,
    #[serde(default = "default_true")]
    pub show_preview: bool,
    #[serde(default = "default_true")]
    pub show_metrics: bool,
    #[serde(default = "default_true")]
    pub show_logs: bool,
    #[serde(default = "default_true")]
    pub show_banner: bool,
}

fn default_true() -> bool {
    true
}

fn default_frame_rate() -> u32 {
    30
}

fn default_tick_rate() -> u64 {
    250
}

fn default_output_buffer() -> usize {
    5000
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            frame_rate: default_frame_rate(),
            tick_rate_ms: default_tick_rate(),
            output_buffer_lines: default_output_buffer(),
            show_preview: default_true(),
            show_metrics: default_true(),
            show_logs: default_true(),
            show_banner: default_true(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Keybind {
    pub key: String,
    #[serde(default)]
    pub modifiers: Vec<String>,
}

impl Keybind {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            modifiers: Vec::new(),
        }
    }

    pub fn with_modifiers(key: impl Into<String>, modifiers: Vec<String>) -> Self {
        Self {
            key: key.into(),
            modifiers,
        }
    }

    pub fn display(&self) -> String {
        if self.modifiers.is_empty() {
            self.key.clone()
        } else {
            format!("{}+{}", self.modifiers.join("+"), self.key)
        }
    }

    pub fn display_short(&self) -> String {
        let key_display = match self.key.as_str() {
            "Up" => "↑".to_string(),
            "Down" => "↓".to_string(),
            "Left" => "←".to_string(),
            "Right" => "→".to_string(),
            "Enter" => "↵".to_string(),
            "Backspace" => "⌫".to_string(),
            "Tab" => "⇥".to_string(),
            "Esc" => "Esc".to_string(),
            k => k.to_string(),
        };
        if self.modifiers.is_empty() {
            key_display
        } else {
            let mods: String = self
                .modifiers
                .iter()
                .map(|m| match m.as_str() {
                    "Control" => "C-".to_string(),
                    "Shift" => "S-".to_string(),
                    "Alt" => "M-".to_string(),
                    _ => format!("{}-", m.chars().next().unwrap_or('?')),
                })
                .collect();
            format!("{}{}", mods, key_display)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keybinds {
    #[serde(default = "default_nav_down")]
    pub nav_down: Keybind,
    #[serde(default = "default_nav_up")]
    pub nav_up: Keybind,
    #[serde(default = "default_nav_first")]
    pub nav_first: Keybind,
    #[serde(default = "default_nav_last")]
    pub nav_last: Keybind,
    #[serde(default = "default_new_agent")]
    pub new_agent: Keybind,
    #[serde(default = "default_delete_agent")]
    pub delete_agent: Keybind,
    #[serde(default = "default_attach")]
    pub attach: Keybind,
    #[serde(default = "default_set_note")]
    pub set_note: Keybind,
    #[serde(default = "default_yank")]
    pub yank: Keybind,
    #[serde(default = "default_pause")]
    pub pause: Keybind,
    #[serde(default = "default_resume")]
    pub resume: Keybind,
    #[serde(default = "default_merge")]
    pub merge: Keybind,
    #[serde(default = "default_push")]
    pub push: Keybind,
    #[serde(default = "default_fetch")]
    pub fetch: Keybind,
    #[serde(default = "default_summary")]
    pub summary: Keybind,
    #[serde(default = "default_toggle_diff")]
    pub toggle_diff: Keybind,
    #[serde(default = "default_toggle_logs")]
    pub toggle_logs: Keybind,
    #[serde(default = "default_open_mr")]
    pub open_mr: Keybind,
    #[serde(default = "default_asana_assign")]
    pub asana_assign: Keybind,
    #[serde(default = "default_asana_open")]
    pub asana_open: Keybind,
    #[serde(default = "default_refresh_all")]
    pub refresh_all: Keybind,
    #[serde(default = "default_toggle_help")]
    pub toggle_help: Keybind,
    #[serde(default = "default_toggle_settings")]
    pub toggle_settings: Keybind,
    #[serde(default = "default_quit")]
    pub quit: Keybind,
    #[serde(default = "default_open_editor")]
    pub open_editor: Keybind,
    #[serde(default = "default_show_tasks")]
    pub show_tasks: Keybind,
    #[serde(default = "default_refresh_task_list")]
    pub refresh_task_list: Keybind,
}

fn default_nav_down() -> Keybind {
    Keybind::new("j")
}
fn default_nav_up() -> Keybind {
    Keybind::new("k")
}
fn default_nav_first() -> Keybind {
    Keybind::new("g")
}
fn default_nav_last() -> Keybind {
    Keybind::with_modifiers("g", vec!["Shift".to_string()])
}
fn default_new_agent() -> Keybind {
    Keybind::new("n")
}
fn default_delete_agent() -> Keybind {
    Keybind::new("d")
}
fn default_attach() -> Keybind {
    Keybind::new("Enter")
}
fn default_set_note() -> Keybind {
    Keybind::with_modifiers("n", vec!["Shift".to_string()])
}
fn default_yank() -> Keybind {
    Keybind::new("y")
}
fn default_pause() -> Keybind {
    Keybind::new("c")
}
fn default_resume() -> Keybind {
    Keybind::new("r")
}
fn default_merge() -> Keybind {
    Keybind::new("m")
}
fn default_push() -> Keybind {
    Keybind::new("p")
}
fn default_fetch() -> Keybind {
    Keybind::new("f")
}
fn default_summary() -> Keybind {
    Keybind::new("s")
}
fn default_toggle_diff() -> Keybind {
    Keybind::new("/")
}
fn default_toggle_logs() -> Keybind {
    Keybind::with_modifiers("l", vec!["Shift".to_string()])
}
fn default_open_mr() -> Keybind {
    Keybind::new("o")
}
fn default_asana_assign() -> Keybind {
    Keybind::new("a")
}
fn default_asana_open() -> Keybind {
    Keybind::with_modifiers("a", vec!["Shift".to_string()])
}
fn default_refresh_all() -> Keybind {
    Keybind::with_modifiers("r", vec!["Shift".to_string()])
}
fn default_toggle_help() -> Keybind {
    Keybind::new("?")
}
fn default_toggle_settings() -> Keybind {
    Keybind::with_modifiers("s", vec!["Shift".to_string()])
}
fn default_quit() -> Keybind {
    Keybind::new("q")
}
fn default_open_editor() -> Keybind {
    Keybind::new("e")
}
fn default_show_tasks() -> Keybind {
    Keybind::new("t")
}
fn default_refresh_task_list() -> Keybind {
    Keybind::new("r")
}

impl Default for Keybinds {
    fn default() -> Self {
        Self {
            nav_down: default_nav_down(),
            nav_up: default_nav_up(),
            nav_first: default_nav_first(),
            nav_last: default_nav_last(),
            new_agent: default_new_agent(),
            delete_agent: default_delete_agent(),
            attach: default_attach(),
            set_note: default_set_note(),
            yank: default_yank(),
            pause: default_pause(),
            resume: default_resume(),
            merge: default_merge(),
            push: default_push(),
            fetch: default_fetch(),
            summary: default_summary(),
            toggle_diff: default_toggle_diff(),
            toggle_logs: default_toggle_logs(),
            open_mr: default_open_mr(),
            asana_assign: default_asana_assign(),
            asana_open: default_asana_open(),
            refresh_all: default_refresh_all(),
            toggle_help: default_toggle_help(),
            toggle_settings: default_toggle_settings(),
            quit: default_quit(),
            open_editor: default_open_editor(),
            show_tasks: default_show_tasks(),
            refresh_task_list: default_refresh_task_list(),
        }
    }
}

impl Keybinds {
    pub fn all_keybinds(&self) -> Vec<(&'static str, &Keybind)> {
        vec![
            ("nav_down", &self.nav_down),
            ("nav_up", &self.nav_up),
            ("nav_first", &self.nav_first),
            ("nav_last", &self.nav_last),
            ("new_agent", &self.new_agent),
            ("delete_agent", &self.delete_agent),
            ("attach", &self.attach),
            ("set_note", &self.set_note),
            ("yank", &self.yank),
            ("pause", &self.pause),
            ("resume", &self.resume),
            ("merge", &self.merge),
            ("push", &self.push),
            ("fetch", &self.fetch),
            ("summary", &self.summary),
            ("toggle_diff", &self.toggle_diff),
            ("toggle_logs", &self.toggle_logs),
            ("open_mr", &self.open_mr),
            ("asana_assign", &self.asana_assign),
            ("asana_open", &self.asana_open),
            ("refresh_all", &self.refresh_all),
            ("toggle_help", &self.toggle_help),
            ("toggle_settings", &self.toggle_settings),
            ("quit", &self.quit),
            ("open_editor", &self.open_editor),
            ("show_tasks", &self.show_tasks),
            ("refresh_task_list", &self.refresh_task_list),
        ]
    }

    pub fn find_conflicts(&self) -> Vec<(String, String)> {
        let keybinds = self.all_keybinds();
        let mut conflicts = Vec::new();
        for i in 0..keybinds.len() {
            for j in (i + 1)..keybinds.len() {
                let (name1, kb1) = keybinds[i];
                let (name2, kb2) = keybinds[j];
                if kb1 == kb2 {
                    conflicts.push((name1.to_string(), name2.to_string()));
                }
            }
        }
        conflicts
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    #[serde(default = "default_agent_poll")]
    pub agent_poll_ms: u64,
    #[serde(default = "default_git_refresh")]
    pub git_refresh_secs: u64,
    #[serde(default = "default_gitlab_refresh")]
    pub gitlab_refresh_secs: u64,
    #[serde(default = "default_github_refresh")]
    pub github_refresh_secs: u64,
    #[serde(default = "default_codeberg_refresh")]
    pub codeberg_refresh_secs: u64,
}

fn default_agent_poll() -> u64 {
    500
}

fn default_git_refresh() -> u64 {
    30
}

fn default_gitlab_refresh() -> u64 {
    60
}

fn default_github_refresh() -> u64 {
    60
}

fn default_codeberg_refresh() -> u64 {
    60
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            agent_poll_ms: default_agent_poll(),
            git_refresh_secs: default_git_refresh(),
            gitlab_refresh_secs: default_gitlab_refresh(),
            github_refresh_secs: default_github_refresh(),
            codeberg_refresh_secs: default_codeberg_refresh(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content =
                std::fs::read_to_string(&config_path).context("Failed to read config file")?;
            toml::from_str(&content).context("Failed to parse config file")
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        Self::ensure_config_dir()?;
        let config_path = Self::config_path()?;
        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;
        std::fs::write(&config_path, content).context("Failed to write config file")
    }

    pub fn config_dir() -> Result<PathBuf> {
        let dir = dirs::home_dir()
            .context("Could not find home directory")?
            .join(".grove");
        Ok(dir)
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    pub fn ensure_config_dir() -> Result<PathBuf> {
        let dir = Self::config_dir()?;
        if !dir.exists() {
            std::fs::create_dir_all(&dir).context("Failed to create config directory")?;
        }
        Ok(dir)
    }

    pub fn gitlab_token() -> Option<String> {
        std::env::var("GITLAB_TOKEN").ok()
    }

    pub fn github_token() -> Option<String> {
        std::env::var("GITHUB_TOKEN").ok()
    }

    pub fn asana_token() -> Option<String> {
        std::env::var("ASANA_TOKEN").ok()
    }

    pub fn notion_token() -> Option<String> {
        std::env::var("NOTION_TOKEN").ok()
    }

    pub fn clickup_token() -> Option<String> {
        std::env::var("CLICKUP_TOKEN").ok()
    }

    pub fn airtable_token() -> Option<String> {
        std::env::var("AIRTABLE_TOKEN").ok()
    }

    pub fn linear_token() -> Option<String> {
        std::env::var("LINEAR_TOKEN").ok()
    }

    pub fn codeberg_token() -> Option<String> {
        std::env::var("CODEBERG_TOKEN").ok()
    }

    pub fn woodpecker_token() -> Option<String> {
        std::env::var("WOODPECKER_TOKEN").ok()
    }

    pub fn exists() -> bool {
        Self::config_dir().map(|d| d.exists()).unwrap_or(false)
    }

    pub fn worktree_base_path(&self, repo_path: &str) -> PathBuf {
        match self.global.worktree_location {
            WorktreeLocation::Project => PathBuf::from(repo_path).join(".worktrees"),
            WorktreeLocation::Home => {
                let repo_hash = Self::repo_hash(repo_path);
                Self::config_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join("worktrees")
                    .join(repo_hash)
            }
        }
    }

    fn repo_hash(repo_path: &str) -> String {
        let mut hasher = DefaultHasher::new();
        repo_path.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoConfig {
    #[serde(default)]
    pub git: RepoGitConfig,
    #[serde(default)]
    pub project_mgmt: RepoProjectMgmtConfig,
    #[serde(default)]
    pub prompts: PromptsConfig,
    #[serde(default)]
    pub dev_server: DevServerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoProjectMgmtConfig {
    #[serde(default)]
    pub provider: ProjectMgmtProvider,
    #[serde(default)]
    pub asana: RepoAsanaConfig,
    #[serde(default)]
    pub notion: RepoNotionConfig,
    #[serde(default)]
    pub clickup: RepoClickUpConfig,
    #[serde(default)]
    pub airtable: RepoAirtableConfig,
    #[serde(default)]
    pub linear: RepoLinearConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoNotionConfig {
    pub database_id: Option<String>,
    pub status_property_name: Option<String>,
    pub in_progress_option: Option<String>,
    pub done_option: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoClickUpConfig {
    pub list_id: Option<String>,
    pub in_progress_status: Option<String>,
    pub done_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoAirtableConfig {
    pub base_id: Option<String>,
    pub table_name: Option<String>,
    pub status_field_name: Option<String>,
    pub in_progress_option: Option<String>,
    pub done_option: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoLinearConfig {
    pub team_id: Option<String>,
    pub in_progress_state: Option<String>,
    pub done_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptsConfig {
    pub summary_prompt: Option<String>,
    pub merge_prompt: Option<String>,
    pub push_prompt_opencode: Option<String>,
    pub push_prompt_codex: Option<String>,
    pub push_prompt_gemini: Option<String>,
}

impl PromptsConfig {
    pub fn get_summary_prompt(&self) -> &str {
        self.summary_prompt.as_deref().unwrap_or(
            "Please provide a brief, non-technical summary of the work done on this branch. \
             Format it as 1-5 bullet points suitable for sharing with non-technical colleagues on Slack. \
             Focus on what was accomplished and why, not implementation details. \
             Keep each bullet point to one sentence.",
        )
    }

    pub fn get_merge_prompt(&self, main_branch: &str) -> String {
        self.merge_prompt
            .as_deref()
            .map(|p| p.replace("{main_branch}", main_branch))
            .unwrap_or_else(|| {
                format!(
                    "Please merge {} into this branch. Handle any merge conflicts if they arise.",
                    main_branch
                )
            })
    }

    pub fn get_push_prompt(&self, agent: &AiAgent) -> Option<String> {
        match agent {
            AiAgent::ClaudeCode => None,
            AiAgent::Opencode => Some(self.push_prompt_opencode.clone().unwrap_or_else(|| {
                "Review the changes, then commit and push them to the remote branch.".to_string()
            })),
            AiAgent::Codex => Some(
                self.push_prompt_codex
                    .clone()
                    .unwrap_or_else(|| "Please commit and push these changes".to_string()),
            ),
            AiAgent::Gemini => Some(
                self.push_prompt_gemini
                    .clone()
                    .unwrap_or_else(|| "Please commit and push these changes".to_string()),
            ),
        }
    }

    pub fn get_push_prompt_for_display(&self, agent: &AiAgent) -> Option<&str> {
        match agent {
            AiAgent::ClaudeCode => None,
            AiAgent::Opencode => {
                Some(self.push_prompt_opencode.as_deref().unwrap_or(
                    "Review the changes, then commit and push them to the remote branch.",
                ))
            }
            AiAgent::Codex => Some(
                self.push_prompt_codex
                    .as_deref()
                    .unwrap_or("Please commit and push these changes"),
            ),
            AiAgent::Gemini => Some(
                self.push_prompt_gemini
                    .as_deref()
                    .unwrap_or("Please commit and push these changes"),
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoGitConfig {
    #[serde(default)]
    pub provider: GitProvider,
    #[serde(default = "default_branch_prefix")]
    pub branch_prefix: String,
    #[serde(default = "default_main_branch")]
    pub main_branch: String,
    #[serde(default)]
    pub gitlab: RepoGitLabConfig,
    #[serde(default)]
    pub github: RepoGitHubConfig,
    #[serde(default)]
    pub codeberg: RepoCodebergConfig,
}

fn default_branch_prefix() -> String {
    "feature/".to_string()
}

fn default_main_branch() -> String {
    "main".to_string()
}

impl Default for RepoGitConfig {
    fn default() -> Self {
        Self {
            provider: GitProvider::default(),
            branch_prefix: default_branch_prefix(),
            main_branch: default_main_branch(),
            gitlab: RepoGitLabConfig::default(),
            github: RepoGitHubConfig::default(),
            codeberg: RepoCodebergConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoGitLabConfig {
    pub project_id: Option<u64>,
    #[serde(default = "default_gitlab_url")]
    pub base_url: String,
}

impl Default for RepoGitLabConfig {
    fn default() -> Self {
        Self {
            project_id: None,
            base_url: default_gitlab_url(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoGitHubConfig {
    pub owner: Option<String>,
    pub repo: Option<String>,
}

fn default_codeberg_url() -> String {
    "https://codeberg.org".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoCodebergConfig {
    pub owner: Option<String>,
    pub repo: Option<String>,
    #[serde(default = "default_codeberg_url")]
    pub base_url: String,
    #[serde(default)]
    pub ci_provider: CodebergCiProvider,
    #[serde(default)]
    pub woodpecker_repo_id: Option<u64>,
}

impl Default for RepoCodebergConfig {
    fn default() -> Self {
        Self {
            owner: None,
            repo: None,
            base_url: default_codeberg_url(),
            ci_provider: CodebergCiProvider::default(),
            woodpecker_repo_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoAsanaConfig {
    pub project_gid: Option<String>,
    pub in_progress_section_gid: Option<String>,
    pub done_section_gid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DevServerConfig {
    pub command: Option<String>,
    #[serde(default)]
    pub run_before: Vec<String>,
    #[serde(default)]
    pub working_dir: String,
    pub port: Option<u16>,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub worktree_symlinks: Vec<String>,
}

impl RepoConfig {
    pub fn load(repo_path: &str) -> Result<Self> {
        let config_path = Self::config_path(repo_path)?;

        if config_path.exists() {
            let content =
                std::fs::read_to_string(&config_path).context("Failed to read repo config")?;

            if let Ok(config) = toml::from_str::<RepoConfig>(&content) {
                return Ok(config);
            }

            #[derive(Deserialize)]
            struct LegacyRepoConfig {
                git: RepoGitConfig,
                asana: RepoAsanaConfig,
                prompts: PromptsConfig,
            }

            if let Ok(legacy) = toml::from_str::<LegacyRepoConfig>(&content) {
                return Ok(RepoConfig {
                    git: legacy.git,
                    project_mgmt: RepoProjectMgmtConfig {
                        provider: ProjectMgmtProvider::Asana,
                        asana: legacy.asana,
                        notion: RepoNotionConfig::default(),
                        clickup: RepoClickUpConfig::default(),
                        airtable: RepoAirtableConfig::default(),
                        linear: RepoLinearConfig::default(),
                    },
                    prompts: legacy.prompts,
                    dev_server: DevServerConfig::default(),
                });
            }

            anyhow::bail!("Failed to parse repo config (neither new nor legacy format)")
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, repo_path: &str) -> Result<()> {
        let config_dir = Self::config_dir(repo_path)?;
        if !config_dir.exists() {
            std::fs::create_dir_all(&config_dir).context("Failed to create .grove directory")?;
        }
        let config_path = Self::config_path(repo_path)?;
        let content = toml::to_string_pretty(self).context("Failed to serialize repo config")?;
        std::fs::write(&config_path, content).context("Failed to write repo config")
    }

    fn config_dir(repo_path: &str) -> Result<PathBuf> {
        Ok(PathBuf::from(repo_path).join(".grove"))
    }

    pub fn config_path(repo_path: &str) -> Result<PathBuf> {
        Ok(Self::config_dir(repo_path)?.join("project.toml"))
    }
}
