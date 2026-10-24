//! Harness and tool doctor.
//!
//! Answers "what is actually installed?" before a session fails: every registered adapter is asked for
//! its own [`Detection`], each harness binary is resolved from settings (a configured path) or `PATH`,
//! and `--version` is run with a timeout. `node`, `git` and `gh` are probed the same way. Nothing here
//! is fatal: a missing, failing or hung binary becomes `installed: false` plus a human-readable reason,
//! and the Settings panel still renders.
//!
//! Detection never blocks starting a session (see `architecture.md` §The adapter contract).

use crate::adapters::Detection;
use crate::paths::display_path;
use crate::sessions::event::now;
use crate::sessions::manager::SessionManager;
use crate::settings::SettingsService;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// How long `--version` gets before the process is killed and reported as timed out.
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// One harness row in the doctor table. Pinned by `contract/fixtures/doctor.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessHealth {
    pub id: String,
    pub label: String,
    pub installed: bool,
    /// First non-empty line of `--version`, or the adapter's own version for a builtin harness.
    pub version: Option<String>,
    /// Forward-slash path the binary was resolved to. `builtin:*` for an in-process harness.
    pub path: Option<String>,
    /// Why it is missing or degraded. `None` when everything is healthy.
    pub detail: Option<String>,
}

/// One tool row (`node`, `git`, `gh`). Pinned by `contract/fixtures/doctor.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolHealth {
    pub id: String,
    pub label: String,
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub detail: Option<String>,
}

/// The full doctor report. Pinned by `contract/fixtures/doctor.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub harnesses: Vec<HarnessHealth>,
    pub tools: Vec<ToolHealth>,
    /// RFC 3339, so the panel can say how stale a report is.
    pub checked_at: String,
}

/// Doctor knobs. `path_env` is the test seam (and a future "custom environment" setting): `None`
/// uses the process `PATH`, so production never needs to touch this struct.
#[derive(Debug, Clone)]
pub struct DoctorConfig {
    pub path_env: Option<String>,
    /// Extensions tried for an extension-less program name on Windows. `None` reads `PATHEXT`.
    pub path_extensions: Option<Vec<String>>,
    pub timeout: Duration,
}

impl Default for DoctorConfig {
    fn default() -> Self {
        Self {
            path_env: None,
            path_extensions: None,
            timeout: DEFAULT_PROBE_TIMEOUT,
        }
    }
}

/// The doctor service. Holds the two things it must not re-implement: adapter detection and settings.
pub struct HarnessDoctor {
    settings: Arc<SettingsService>,
    sessions: Arc<SessionManager>,
    config: DoctorConfig,
}

impl HarnessDoctor {
    pub fn new(settings: Arc<SettingsService>, sessions: Arc<SessionManager>) -> Self {
        Self::with_config(settings, sessions, DoctorConfig::default())
    }

    pub fn with_config(
        settings: Arc<SettingsService>,
        sessions: Arc<SessionManager>,
        config: DoctorConfig,
    ) -> Self {
        Self {
            settings,
            sessions,
            config,
        }
    }

    /// Probe everything. Reading settings is best-effort: a broken settings row must not take the
    /// doctor down, so it falls back to defaults (PATH resolution, no overrides).
    pub async fn report(&self) -> DoctorReport {
        let settings = self.settings.get().unwrap_or_default();
        let adapters = self.sessions.harnesses();
        let detections = self.sessions.detect_all();

        let mut harnesses = Vec::with_capacity(adapters.len());
        for (adapter, detection) in adapters.iter().zip(detections.iter()) {
            let configured = settings
                .harness_paths
                .get(&adapter.id)
                .map(|path| path.trim())
                .filter(|path| !path.is_empty());
            harnesses.push(
                self.harness_health(&adapter.id, detection, configured)
                    .await,
            );
        }

        let mut tools = Vec::new();
        for (id, label) in TOOLS {
            tools.push(self.tool_health(id, label).await);
        }

        DoctorReport {
            harnesses,
            tools,
            checked_at: now(),
        }
    }

    async fn harness_health(
        &self,
        id: &str,
        detection: &Detection,
        configured: Option<&str>,
    ) -> HarnessHealth {
        let label = harness_label(id);

        // An explicit override is a promise: never silently fall back to PATH when it is wrong.
        if let Some(raw) = configured {
            let shown = clean_path(raw);
            let Some(path) = self.resolve_configured(raw) else {
                return HarnessHealth {
                    id: id.to_string(),
                    label,
                    installed: false,
                    version: None,
                    path: Some(shown.clone()),
                    detail: Some(format!("configured path does not exist: {shown}")),
                };
            };
            let probe = self.probe(&path).await;
            return HarnessHealth {
                id: id.to_string(),
                label,
                installed: probe.installed,
                version: probe.version,
                path: Some(probe.path.unwrap_or_else(|| display_path(&path))),
                detail: probe.detail,
            };
        }

        // A builtin harness lives in-process: the adapter's own detection is the whole truth.
        if detection.installed && is_builtin(detection) {
            return detection_health(id, label, detection);
        }

        if let Some(path) = self.find_on_path(id) {
            let probe = self.probe(&path).await;
            return HarnessHealth {
                id: id.to_string(),
                label,
                installed: probe.installed,
                version: probe.version,
                path: Some(probe.path.unwrap_or_else(|| display_path(&path))),
                detail: probe.detail,
            };
        }

        if detection.installed {
            // The adapter knows a location PATH does not (an SDK, a bundled binary); trust it.
            return detection_health(id, label, detection);
        }

        let mut health = detection_health(id, label, detection);
        if health.detail.is_none() {
            health.detail = Some(format!("{id} is not installed or not on PATH"));
        }
        health
    }

    async fn tool_health(&self, id: &str, label: &str) -> ToolHealth {
        let Some(path) = self.find_on_path(id) else {
            return ToolHealth {
                id: id.to_string(),
                label: label.to_string(),
                installed: false,
                version: None,
                path: None,
                detail: Some(format!("{id} is not installed or not on PATH")),
            };
        };
        let probe = self.probe(&path).await;
        ToolHealth {
            id: id.to_string(),
            label: label.to_string(),
            installed: probe.installed,
            version: probe.version,
            path: Some(probe.path.unwrap_or_else(|| display_path(&path))),
            detail: probe.detail,
        }
    }

    /// Run `<program> --version` with the configured timeout. A hung process is killed and reaped
    /// before returning, so the doctor task never leaves a child behind.
    async fn probe(&self, program: &Path) -> Probe {
        let shown = display_path(program);
        let mut command = tokio::process::Command::new(program);
        command
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                return Probe {
                    installed: false,
                    version: None,
                    path: Some(shown),
                    detail: Some(format!("failed to run: {error}")),
                }
            }
        };

        let status = match tokio::time::timeout(self.config.timeout, child.wait()).await {
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Probe {
                    installed: false,
                    version: None,
                    path: Some(shown),
                    detail: Some(format!(
                        "timed out after {}ms",
                        self.config.timeout.as_millis()
                    )),
                };
            }
            Ok(Err(error)) => {
                return Probe {
                    installed: false,
                    version: None,
                    path: Some(shown),
                    detail: Some(format!("failed to run: {error}")),
                }
            }
            Ok(Ok(status)) => status,
        };

        // `--version` output is tiny, so reading after the process exits cannot deadlock on a full
        // pipe; the cap keeps a hostile binary from filling memory.
        let stdout = read_capped(child.stdout.take()).await;
        let stderr = read_capped(child.stderr.take()).await;
        let code = status.code().unwrap_or(-1);
        if status.success() {
            let version = version_line(&stdout, &stderr);
            Probe {
                installed: true,
                path: Some(shown),
                detail: version
                    .is_none()
                    .then(|| "reported no version output".to_string()),
                version,
            }
        } else {
            let reason = stderr_tail(&stderr);
            Probe {
                installed: false,
                version: None,
                path: Some(shown),
                detail: Some(if reason.is_empty() {
                    format!("exited with status {code}")
                } else {
                    format!("exited with status {code}: {reason}")
                }),
            }
        }
    }

    /// An explicit configured value: a path when it looks like one, otherwise a program name resolved
    /// on PATH.
    fn resolve_configured(&self, raw: &str) -> Option<PathBuf> {
        let cleaned = clean_path(raw);
        if cleaned.is_empty() {
            return None;
        }
        let path = PathBuf::from(&cleaned);
        if path.is_file() {
            return Some(path);
        }
        if cleaned.contains('/') || cleaned.contains('\\') {
            return None;
        }
        self.find_on_path(&cleaned)
    }

    fn find_on_path(&self, program: &str) -> Option<PathBuf> {
        let path_env = self
            .config
            .path_env
            .clone()
            .or_else(|| std::env::var("PATH").ok())?;
        let extensions = self.extensions();
        for directory in std::env::split_paths(&path_env) {
            if directory.as_os_str().is_empty() {
                continue;
            }
            let direct = directory.join(program);
            if direct.is_file() {
                return Some(direct);
            }
            for extension in &extensions {
                let candidate = directory.join(format!("{program}{extension}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    fn extensions(&self) -> Vec<String> {
        if let Some(extensions) = &self.config.path_extensions {
            return extensions
                .iter()
                .map(|extension| extension.to_ascii_lowercase())
                .collect();
        }
        if !cfg!(windows) {
            return Vec::new();
        }
        std::env::var("PATHEXT")
            .ok()
            .map(|value| {
                value
                    .split(';')
                    .filter(|extension| !extension.is_empty())
                    .map(|extension| extension.to_ascii_lowercase())
                    .collect::<Vec<_>>()
            })
            .filter(|extensions| !extensions.is_empty())
            .unwrap_or_else(|| [".com", ".exe", ".bat", ".cmd"].map(String::from).to_vec())
    }
}

/// `node`, `git` and `gh`: the tools a session or a PR needs before Grove can promise anything.
const TOOLS: [(&str, &str); 3] = [("node", "Node.js"), ("git", "Git"), ("gh", "GitHub CLI")];

struct Probe {
    installed: bool,
    version: Option<String>,
    path: Option<String>,
    detail: Option<String>,
}

/// Version probes are bounded: a program that prints a novel must not balloon the report.
const MAX_VERSION_BYTES: u64 = 64 * 1024;

async fn read_capped<R: tokio::io::AsyncRead + Unpin>(stream: Option<R>) -> String {
    use tokio::io::AsyncReadExt;
    let Some(stream) = stream else {
        return String::new();
    };
    let mut bytes = Vec::new();
    let _ = stream.take(MAX_VERSION_BYTES).read_to_end(&mut bytes).await;
    String::from_utf8_lossy(&bytes).into_owned()
}

fn detection_health(id: &str, label: String, detection: &Detection) -> HarnessHealth {
    HarnessHealth {
        id: id.to_string(),
        label,
        installed: detection.installed,
        version: detection.version.clone(),
        path: detection.path.clone(),
        detail: detection.detail.clone(),
    }
}

fn is_builtin(detection: &Detection) -> bool {
    detection
        .path
        .as_deref()
        .is_some_and(|path| path.starts_with("builtin:"))
}

fn harness_label(id: &str) -> String {
    match id {
        "claude" => "Claude Code",
        "codex" => "Codex",
        "acp" => "ACP",
        "opencode" => "OpenCode",
        "pi" => "pi",
        "mock" => "Mock (builtin)",
        other => other,
    }
    .to_string()
}

fn clean_path(raw: &str) -> String {
    raw.trim().trim_matches('"').trim().to_string()
}

fn version_line(stdout: &str, stderr: &str) -> Option<String> {
    let raw = if stdout.trim().is_empty() {
        stderr
    } else {
        stdout
    };
    let line = raw.lines().map(str::trim).find(|line| !line.is_empty())?;
    let mut version: String = line.chars().take(160).collect();
    if line.chars().count() > 160 {
        version.push('…');
    }
    Some(version)
}

fn stderr_tail(stderr: &str) -> String {
    let trimmed = stderr.trim();
    let characters: Vec<char> = trimmed.chars().collect();
    if characters.len() <= 400 {
        return trimmed.to_string();
    }
    characters[characters.len() - 400..].iter().collect()
}
