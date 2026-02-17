use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub gitlab: GitLabConfig,
    #[serde(default)]
    pub asana: AsanaConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub performance: PerformanceConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            gitlab: GitLabConfig::default(),
            asana: AsanaConfig::default(),
            ui: UiConfig::default(),
            performance: PerformanceConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsanaConfig {
    pub project_gid: Option<String>,
    pub in_progress_section_gid: Option<String>,
    pub done_section_gid: Option<String>,
    #[serde(default = "default_asana_refresh")]
    pub refresh_secs: u64,
}

fn default_asana_refresh() -> u64 {
    120
}

impl Default for AsanaConfig {
    fn default() -> Self {
        Self {
            project_gid: None,
            in_progress_section_gid: None,
            done_section_gid: None,
            refresh_secs: default_asana_refresh(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabConfig {
    #[serde(default = "default_gitlab_url")]
    pub base_url: String,
    pub project_id: Option<u64>,
    #[serde(default = "default_main_branch")]
    pub main_branch: String,
}

fn default_gitlab_url() -> String {
    "https://gitlab.com".to_string()
}

fn default_main_branch() -> String {
    "main".to_string()
}

impl Default for GitLabConfig {
    fn default() -> Self {
        Self {
            base_url: default_gitlab_url(),
            project_id: None,
            main_branch: default_main_branch(),
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

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            agent_poll_ms: default_agent_poll(),
            git_refresh_secs: default_git_refresh(),
            gitlab_refresh_secs: default_gitlab_refresh(),
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

    pub fn asana_token() -> Option<String> {
        std::env::var("ASANA_TOKEN").ok()
    }
}
