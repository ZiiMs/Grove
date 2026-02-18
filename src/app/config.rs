use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
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
    Bitbucket,
}

impl GitProvider {
    pub fn display_name(&self) -> &'static str {
        match self {
            GitProvider::GitLab => "GitLab",
            GitProvider::GitHub => "GitHub",
            GitProvider::Bitbucket => "Bitbucket",
        }
    }

    pub fn all() -> &'static [GitProvider] {
        &[
            GitProvider::GitLab,
            GitProvider::GitHub,
            GitProvider::Bitbucket,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalConfig {
    #[serde(default)]
    pub ai_agent: AiAgent,
    #[serde(default)]
    pub log_level: LogLevel,
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
    pub ui: UiConfig,
    #[serde(default)]
    pub performance: PerformanceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsanaConfig {
    #[serde(default = "default_asana_refresh")]
    pub refresh_secs: u64,
}

fn default_asana_refresh() -> u64 {
    120
}

impl Default for AsanaConfig {
    fn default() -> Self {
        Self {
            refresh_secs: default_asana_refresh(),
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
        }
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

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            agent_poll_ms: default_agent_poll(),
            git_refresh_secs: default_git_refresh(),
            gitlab_refresh_secs: default_gitlab_refresh(),
            github_refresh_secs: default_github_refresh(),
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
            .join(".flock");
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoConfig {
    #[serde(default)]
    pub git: RepoGitConfig,
    #[serde(default)]
    pub asana: RepoAsanaConfig,
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
    pub worktree_symlinks: Vec<String>,
    #[serde(default)]
    pub gitlab: RepoGitLabConfig,
    #[serde(default)]
    pub github: RepoGitHubConfig,
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
            worktree_symlinks: Vec::new(),
            gitlab: RepoGitLabConfig::default(),
            github: RepoGitHubConfig::default(),
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoAsanaConfig {
    pub project_gid: Option<String>,
    pub in_progress_section_gid: Option<String>,
    pub done_section_gid: Option<String>,
}

impl RepoConfig {
    pub fn load(repo_path: &str) -> Result<Self> {
        let config_path = Self::config_path(repo_path)?;

        if config_path.exists() {
            let content =
                std::fs::read_to_string(&config_path).context("Failed to read repo config")?;
            toml::from_str(&content).context("Failed to parse repo config")
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, repo_path: &str) -> Result<()> {
        let config_dir = Self::config_dir(repo_path)?;
        if !config_dir.exists() {
            std::fs::create_dir_all(&config_dir).context("Failed to create .flock directory")?;
        }
        let config_path = Self::config_path(repo_path)?;
        let content = toml::to_string_pretty(self).context("Failed to serialize repo config")?;
        std::fs::write(&config_path, content).context("Failed to write repo config")
    }

    fn config_dir(repo_path: &str) -> Result<PathBuf> {
        Ok(PathBuf::from(repo_path).join(".flock"))
    }

    fn config_path(repo_path: &str) -> Result<PathBuf> {
        Ok(Self::config_dir(repo_path)?.join("project.toml"))
    }
}
