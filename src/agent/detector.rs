use regex::Regex;
use std::sync::LazyLock;

use super::{AgentStatus, StatusReason};
use crate::app::config::AiAgent;
use crate::gitlab::{MergeRequestStatus, PipelineStatus};
use chrono::Utc;

#[derive(Debug, Clone, PartialEq)]
pub struct StatusDetection {
    pub status: AgentStatus,
    pub reason: Option<String>,
    pub pattern: Option<String>,
}

impl StatusDetection {
    pub fn new(status: AgentStatus) -> Self {
        Self {
            status,
            reason: None,
            pattern: None,
        }
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn with_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.pattern = Some(pattern.into());
        self
    }

    pub fn to_status_reason(&self) -> Option<StatusReason> {
        self.reason.as_ref().map(|reason| StatusReason {
            status: self.status.clone(),
            reason: reason.clone(),
            pattern: self.pattern.clone(),
            timestamp: Utc::now(),
        })
    }
}

/// Classification of the foreground process in the tmux pane.
/// Used as ground truth for status detection.
#[derive(Debug, Clone, PartialEq)]
pub enum ForegroundProcess {
    /// Claude Code is alive (node, claude, npx)
    ClaudeRunning,
    /// Opencode is alive (node, opencode, npx)
    OpencodeRunning,
    /// Codex is alive (codex)
    CodexRunning,
    /// Gemini is alive (node, gemini)
    GeminiRunning,
    /// At a shell prompt (bash, zsh, sh, fish, dash)
    Shell,
    /// AI agent spawned a subprocess (cargo, git, python, etc.)
    OtherProcess(String),
    /// Could not determine (tmux error or unavailable)
    Unknown,
}

impl ForegroundProcess {
    /// Classify a process command name into a `ForegroundProcess` variant.
    /// Uses the configured agent type to determine which AI process names to recognize.
    pub fn from_command_for_agent(cmd: &str, agent_type: AiAgent) -> Self {
        let cmd_lower = cmd.to_lowercase();
        let binary = cmd_lower.rsplit('/').next().unwrap_or(&cmd_lower);

        if agent_type.process_names().contains(&binary) {
            return match agent_type {
                AiAgent::ClaudeCode => ForegroundProcess::ClaudeRunning,
                AiAgent::Opencode => ForegroundProcess::OpencodeRunning,
                AiAgent::Codex => ForegroundProcess::CodexRunning,
                AiAgent::Gemini => ForegroundProcess::GeminiRunning,
            };
        }

        match binary {
            "bash" | "zsh" | "sh" | "fish" | "dash" => ForegroundProcess::Shell,
            "" => ForegroundProcess::Unknown,
            other => ForegroundProcess::OtherProcess(other.to_string()),
        }
    }

    /// Legacy method for backward compatibility (assumes Claude Code)
    pub fn from_command(cmd: &str) -> Self {
        Self::from_command_for_agent(cmd, AiAgent::ClaudeCode)
    }

    /// Check if this represents any AI agent running
    pub fn is_agent_running(&self) -> bool {
        matches!(
            self,
            ForegroundProcess::ClaudeRunning
                | ForegroundProcess::OpencodeRunning
                | ForegroundProcess::CodexRunning
                | ForegroundProcess::GeminiRunning
        )
    }
}

/// Pattern to strip ANSI escape codes
static ANSI_ESCAPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]|\x1b\].*?\x07").unwrap());

/// Pattern to detect GitLab MR URLs in output
static MR_URL_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https://[^/]+/[^/]+/[^/]+/-/merge_requests/(\d+)").unwrap());

/// Braille spinner characters used by Claude Code
static SPINNER_CHARS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏◐◓◑◒⣾⣽⣻⢿⡿⣟⣯⣷]").unwrap());

/// Tool execution patterns (Claude Code shows these when running tools)
static TOOL_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // Tool names with trailing context
        Regex::new(r"⏺\s*(Read|Write|Edit|Bash|Glob|Grep|Task|WebFetch|WebSearch)").unwrap(),
        // Active status messages
        Regex::new(r"(?i)^(reading|writing|editing|searching|running|executing|thinking|analyzing|processing|fetching|installing|building|compiling|testing)").unwrap(),
    ]
});

/// Patterns indicating Claude is asking a question or needs permission
static QUESTION_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // Yes/No prompts
        Regex::new(r"\(y/n\)").unwrap(),
        Regex::new(r"\[y/N\]").unwrap(),
        Regex::new(r"\[Y/n\]").unwrap(),
        Regex::new(r"\[yes/no\]").unwrap(),
        // Permission prompts (Claude Code specific)
        Regex::new(r"Allow\s*(this|once|always)?\s*\?").unwrap(),
        Regex::new(r"Do you want to (allow|proceed|continue)").unwrap(),
        // Bash command confirmation
        Regex::new(r"Run this command\?").unwrap(),
        Regex::new(r"Execute\?").unwrap(),
        // Plan mode
        Regex::new(r"Ready to implement\?").unwrap(),
        Regex::new(r"Proceed with").unwrap(),
        // Numbered selection with Claude's indicator (❯ 1. Option text)
        Regex::new(r"❯\s*\d+\.").unwrap(),
        // Keyboard confirmation hints
        Regex::new(r"Enter\s+to\s+confirm").unwrap(),
        Regex::new(r"Esc\s+to\s+cancel").unwrap(),
    ]
});

/// Patterns indicating completion
static COMPLETION_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"[✓✔☑]\s").unwrap(),
        Regex::new(r"(?i)^done\.?\s*$").unwrap(),
        Regex::new(r"(?i)completed successfully").unwrap(),
        Regex::new(r"(?i)finished").unwrap(),
        Regex::new(r"(?i)all tests pass").unwrap(),
    ]
});

/// Patterns indicating an error
static ERROR_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"[✗✘❌]\s").unwrap(),
        Regex::new(r"(?m)^Error:").unwrap(),
        Regex::new(r"(?m)^ERROR:").unwrap(),
        Regex::new(r"(?m)^error\[E\d+\]").unwrap(), // Rust errors
        Regex::new(r"FAILED").unwrap(),
        Regex::new(r"panicked at").unwrap(),
        Regex::new(r"(?i)command failed").unwrap(),
    ]
});

// OpenCode-specific patterns

/// Pattern for OpenCode progress dots (animation when working)
static OPENCODE_PROGRESS_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.{4,}").unwrap());

/// Braille spinner characters (used by various AI tools including OpenCode)
static OPENCODE_SPINNER_CHARS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[⣾⣽⣻⢿⡿⣟⣯⣷⠁⠃⠇⡇⡏⡟⡿⣿⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏]").unwrap());

// Gemini-specific patterns

/// Gemini CLI shows "Action Required" for needs input
static GEMINI_ACTION_REQUIRED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)action\s+required").unwrap());

/// Gemini shows "Waiting for confirmation" dialogs
static GEMINI_WAITING_CONFIRMATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)waiting\s+for\s+confirmation").unwrap());

/// Gemini permission/confirmation prompts with standard patterns
static GEMINI_CONFIRMATION_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"(?i)proceed\??").unwrap(),
        Regex::new(r"(?i)allow\s+this\??").unwrap(),
        Regex::new(r"(?i)confirm\s*\??").unwrap(),
        Regex::new(r"(?i)would\s+you\s+like\s+to").unwrap(),
        Regex::new(r"(?i)do\s+you\s+want\s+to").unwrap(),
    ]
});

/// Gemini question/answer dialog panel (shows "Answer Questions" title)
static GEMINI_ANSWER_QUESTIONS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)answer\s+questions").unwrap());

/// Gemini keyboard hints in question panel (indicates AwaitingInput)
static GEMINI_KEYBOARD_HINTS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)enter\s+to\s+select.*esc\s+to\s+cancel").unwrap());

/// Gemini numbered questions - indicates clarification needed (AwaitingInput)
/// Matches patterns like "   1. Question text?" or "   2. Another question?"
static GEMINI_NUMBERED_QUESTIONS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\d+\.\s+.+\?$").unwrap());

/// Gemini user answers to numbered questions - indicates question phase is over
/// Matches patterns like "   1. New doc" or "   2. Lorem ipsum" (no question mark, short answer)
static GEMINI_NUMBERED_ANSWERS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*[1-4]\.\s+[^?]+$").unwrap());

/// Gemini running indicator: timer format like "(esc to cancel, 15s)" - NOT keyboard hints
static GEMINI_ESC_CANCEL_TIMER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\(esc\s+to\s+cancel,?\s*\d+s").unwrap());

/// Gemini dots spinner (animated square dots pattern)
static GEMINI_DOTS_SPINNER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[⠁⠃⠇⡇⡏⡟⡿⣿]").unwrap());

/// Detect GitLab MR URL in tmux output and return MergeRequestStatus if found.
pub fn detect_mr_url(output: &str) -> Option<MergeRequestStatus> {
    let mut last_match: Option<(u64, String)> = None;

    for cap in MR_URL_PATTERN.captures_iter(output) {
        if let (Some(full_match), Some(iid_match)) = (cap.get(0), cap.get(1)) {
            if let Ok(iid) = iid_match.as_str().parse::<u64>() {
                last_match = Some((iid, full_match.as_str().to_string()));
            }
        }
    }

    last_match.map(|(iid, url)| MergeRequestStatus::Open {
        iid,
        url,
        pipeline: PipelineStatus::None,
    })
}

/// Strip ANSI escape codes from text
fn strip_ansi(text: &str) -> String {
    ANSI_ESCAPE.replace_all(text, "").to_string()
}

/// Pattern to detect collapsed task count like "... +3 completed"
static COLLAPSED_TASKS_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\+(\d+)\s+completed").unwrap());

/// Pattern to detect task summary line like "11 tasks (9 done, 1 in progress, 1 open)"
static TASK_SUMMARY_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+)\s+tasks?\s*\((\d+)\s+done").unwrap());

/// Width of the side panel region (characters from right side of line)
const SIDE_PANEL_WIDTH: usize = 60;

/// Detect checklist progress based on agent type.
/// Routes to the appropriate detection function for each AI agent.
pub fn detect_checklist_progress(output: &str, ai_agent: AiAgent) -> Option<(u32, u32)> {
    match ai_agent {
        AiAgent::ClaudeCode => detect_checklist_claude_code(output),
        AiAgent::Opencode => detect_checklist_opencode(output),
        AiAgent::Codex | AiAgent::Gemini => detect_checklist_generic(output),
    }
}

/// Detect checklist progress from Claude Code output.
/// Checks: task summary lines, collapsed counts, and line-start checkboxes.
fn detect_checklist_claude_code(output: &str) -> Option<(u32, u32)> {
    let clean_output = strip_ansi(output);

    // First, try to find the authoritative task summary line
    // e.g., "11 tasks (9 done, 1 in progress, 1 open)"
    for line in clean_output.lines() {
        if let Some(caps) = TASK_SUMMARY_PATTERN.captures(line) {
            if let (Some(total_match), Some(done_match)) = (caps.get(1), caps.get(2)) {
                if let (Ok(total), Ok(done)) = (
                    total_match.as_str().parse::<u32>(),
                    done_match.as_str().parse::<u32>(),
                ) {
                    return Some((done, total));
                }
            }
        }
    }

    let mut completed = 0u32;
    let mut total = 0u32;

    for line in clean_output.lines() {
        let trimmed = line.trim();

        // Check for collapsed tasks line like "... +3 completed"
        if let Some(caps) = COLLAPSED_TASKS_PATTERN.captures(trimmed) {
            if let Some(count_match) = caps.get(1) {
                if let Ok(count) = count_match.as_str().parse::<u32>() {
                    completed += count;
                    total += count;
                    continue;
                }
            }
        }

        // Check line start for Claude Code style checkboxes
        // Tree chars: │ ├ └ ─ followed by space, then the checkbox
        let check_part = trimmed.trim_start_matches(['│', '├', '└', '─', ' ']);

        if check_part.starts_with("[✓]")
            || check_part.starts_with("[✔]")
            || check_part.starts_with("[✅]")
        {
            completed += 1;
            total += 1;
        } else if check_part.starts_with("[•]")
            || check_part.starts_with("[○]")
            || check_part.starts_with("[ ]")
        {
            total += 1;
        }
        // Check for completed items (checkmarks) - various Unicode checkmarks
        else if check_part.starts_with('✓')
            || check_part.starts_with('✔')
            || check_part.starts_with('☑')
            || check_part.starts_with('✅')
        {
            completed += 1;
            total += 1;
        }
        // Check for in-progress items (filled shapes) or not-started items (empty shapes)
        else if check_part.starts_with('◼')
            || check_part.starts_with('■')
            || check_part.starts_with('▪')
            || check_part.starts_with('●')
            || check_part.starts_with('◻')
            || check_part.starts_with('□')
            || check_part.starts_with('☐')
            || check_part.starts_with('○')
        {
            total += 1;
        }
    }

    if total > 0 {
        Some((completed, total))
    } else {
        None
    }
}

/// Detect checklist progress from OpenCode side panel.
/// Only checks the rightmost portion of lines where the side panel is rendered.
fn detect_checklist_opencode(output: &str) -> Option<(u32, u32)> {
    let clean_output = strip_ansi(output);

    let mut completed = 0u32;
    let mut total = 0u32;

    for line in clean_output.lines() {
        let trimmed = line.trim();
        let chars: Vec<char> = trimmed.chars().collect();

        // Extract the side panel region (rightmost chars) to avoid counting
        // todos mentioned in chat conversation vs side panel display
        // Use char-based indexing to handle multi-byte UTF-8 characters
        let side_panel: String = if chars.len() > SIDE_PANEL_WIDTH {
            chars[chars.len() - SIDE_PANEL_WIDTH..].iter().collect()
        } else {
            trimmed.to_string()
        };

        // Simple string matching - avoid regex for performance
        if side_panel.contains("[✓]") || side_panel.contains("[✔]") || side_panel.contains("[✅]")
        {
            completed += 1;
            total += 1;
        } else if side_panel.contains("[•]")
            || side_panel.contains("[○]")
            || side_panel.contains("[ ]")
        {
            total += 1;
        }
    }

    if total > 0 {
        Some((completed, total))
    } else {
        None
    }
}

/// Generic checklist detection for Codex/Gemini agents.
/// Uses Claude Code style detection as a fallback.
fn detect_checklist_generic(output: &str) -> Option<(u32, u32)> {
    detect_checklist_claude_code(output)
}

/// Detect the status of a Claude Code agent from its tmux output.
///
/// Priority order:
/// 1. AwaitingInput - if there's a question/permission prompt (highest priority)
/// 2. Running - if there are spinners or active tool indicators (BEFORE prompt check!)
/// 3. Idle - at the prompt ready for new task
/// 4. Error - if there are error indicators
/// 5. Completed - if there are completion indicators
/// 6. Idle - default fallback
pub fn detect_status(output: &str) -> StatusDetection {
    let clean_output = strip_ansi(output);
    let lines: Vec<&str> = clean_output.lines().collect();

    if lines.is_empty() {
        return StatusDetection::new(AgentStatus::Stopped).with_reason("No output captured");
    }

    // Get recent lines for analysis (last 15 lines should capture current state)
    let recent_lines: Vec<&str> = lines.iter().rev().take(15).cloned().collect();
    let recent_text = recent_lines.join("\n");

    // Get the last few lines (where spinners and prompts appear)
    let last_3_lines: Vec<&str> = lines.iter().rev().take(3).cloned().collect();
    let last_3_text = last_3_lines.join("\n");

    // 1. CHECK FOR QUESTIONS/PERMISSION PROMPTS (highest priority)
    for pattern in QUESTION_PATTERNS.iter() {
        if pattern.is_match(&recent_text) {
            return StatusDetection::new(AgentStatus::AwaitingInput)
                .with_reason("Found question/permission prompt")
                .with_pattern("QUESTION_PATTERNS");
        }
    }

    // 2. CHECK FOR RUNNING (before prompt check!)
    if SPINNER_CHARS.is_match(&last_3_text) {
        return StatusDetection::new(AgentStatus::Running)
            .with_reason("Found spinner characters in last 3 lines")
            .with_pattern("SPINNER_CHARS");
    }

    for pattern in TOOL_PATTERNS.iter() {
        if pattern.is_match(&last_3_text) {
            return StatusDetection::new(AgentStatus::Running)
                .with_reason("Found tool execution pattern")
                .with_pattern("TOOL_PATTERNS");
        }
    }

    // 3. CHECK IF AT PROMPT
    let is_at_prompt = lines.iter().rev().take(5).any(|line| {
        let trimmed = line.trim().trim_matches('\u{00A0}');
        if trimmed == ">" || trimmed == "›" || trimmed == "❯" || trimmed == "$" || trimmed == "%"
        {
            return true;
        }
        if trimmed.len() <= 3
            && (trimmed.starts_with('>')
                || trimmed.starts_with('›')
                || trimmed.starts_with('❯')
                || trimmed.starts_with('$')
                || trimmed.starts_with('%'))
        {
            return true;
        }
        if trimmed.starts_with("➜") {
            return true;
        }
        false
    });

    if is_at_prompt {
        return StatusDetection::new(AgentStatus::Idle).with_reason("At prompt, ready for input");
    }

    // 4. CHECK FOR ERRORS
    for pattern in ERROR_PATTERNS.iter() {
        if pattern.is_match(&recent_text) {
            for line in recent_lines.iter() {
                if pattern.is_match(line) {
                    let msg = line.trim().chars().take(40).collect::<String>();
                    return StatusDetection::new(AgentStatus::Error(msg.clone()))
                        .with_reason(format!("Error pattern matched: {}", msg))
                        .with_pattern("ERROR_PATTERNS");
                }
            }
            return StatusDetection::new(AgentStatus::Error("Error detected".to_string()))
                .with_reason("Error pattern matched in output")
                .with_pattern("ERROR_PATTERNS");
        }
    }

    // 5. CHECK FOR COMPLETION
    for pattern in COMPLETION_PATTERNS.iter() {
        if pattern.is_match(&recent_text) {
            return StatusDetection::new(AgentStatus::Completed)
                .with_reason("Found completion pattern")
                .with_pattern("COMPLETION_PATTERNS");
        }
    }

    // 6. DEFAULT
    if clean_output.trim().is_empty() {
        StatusDetection::new(AgentStatus::Stopped).with_reason("Empty output")
    } else {
        StatusDetection::new(AgentStatus::Idle)
            .with_reason("Default: has output but no clear indicators")
    }
}

/// Detect agent status using process-level ground truth combined with text analysis.
///
/// This is more accurate than `detect_status()` alone because it uses the tmux
/// foreground process as definitive signal for whether Claude is running.
pub fn detect_status_with_process(output: &str, foreground: ForegroundProcess) -> StatusDetection {
    match foreground {
        ForegroundProcess::ClaudeRunning => detect_status_claude_running(output),
        ForegroundProcess::OpencodeRunning => detect_status_opencode(output, foreground),
        ForegroundProcess::CodexRunning => detect_status_codex(output, foreground),
        ForegroundProcess::GeminiRunning => detect_status_gemini(output, foreground),
        ForegroundProcess::Shell => detect_status_at_shell(output),
        ForegroundProcess::OtherProcess(p) => detect_status_other_process(output, &p),
        ForegroundProcess::Unknown => detect_status(output),
    }
}

/// Status detection when a subprocess (cargo, git, etc.) is in the foreground.
/// Still checks for input prompts since the AI may be waiting for user response.
fn detect_status_other_process(output: &str, process_name: &str) -> StatusDetection {
    let clean_output = strip_ansi(output);
    let lines: Vec<&str> = clean_output.lines().collect();

    if lines.is_empty() {
        return StatusDetection::new(AgentStatus::Running)
            .with_reason(format!("Subprocess '{}' running, no output", process_name));
    }

    let last_5_lines: Vec<&str> = lines
        .iter()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(5)
        .cloned()
        .collect();
    let last_5_text = last_5_lines.join("\n");

    for pattern in QUESTION_PATTERNS.iter() {
        if pattern.is_match(&last_5_text) {
            return StatusDetection::new(AgentStatus::AwaitingInput)
                .with_reason("Found question pattern in subprocess output")
                .with_pattern("QUESTION_PATTERNS");
        }
    }

    for pattern in ERROR_PATTERNS.iter() {
        if pattern.is_match(&last_5_text) {
            for line in last_5_lines.iter() {
                if pattern.is_match(line) {
                    let msg = line.trim().chars().take(40).collect::<String>();
                    return StatusDetection::new(AgentStatus::Error(msg.clone()))
                        .with_reason(format!("Error in subprocess: {}", msg))
                        .with_pattern("ERROR_PATTERNS");
                }
            }
            return StatusDetection::new(AgentStatus::Error("Error detected".to_string()))
                .with_reason("Error pattern matched in subprocess output")
                .with_pattern("ERROR_PATTERNS");
        }
    }

    StatusDetection::new(AgentStatus::Running)
        .with_reason(format!("Subprocess '{}' in foreground", process_name))
}

/// Status detection when we know Claude (node/npx) is the foreground process.
/// Default is Running (fixes false Idle during silent thinking).
fn detect_status_claude_running(output: &str) -> StatusDetection {
    let clean_output = strip_ansi(output);
    let lines: Vec<&str> = clean_output.lines().collect();

    if lines.is_empty() {
        return StatusDetection::new(AgentStatus::Running)
            .with_reason("Claude process running, no output yet");
    }

    let last_5_lines: Vec<&str> = lines
        .iter()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(5)
        .cloned()
        .collect();
    let last_5_text = last_5_lines.join("\n");

    // 1. Check for questions/permission prompts (highest priority)
    for pattern in QUESTION_PATTERNS.iter() {
        if pattern.is_match(&last_5_text) {
            return StatusDetection::new(AgentStatus::AwaitingInput)
                .with_reason("Found question/permission prompt")
                .with_pattern("QUESTION_PATTERNS");
        }
    }

    // 2. Check for errors in last 5 lines
    for pattern in ERROR_PATTERNS.iter() {
        if pattern.is_match(&last_5_text) {
            for line in last_5_lines.iter() {
                if pattern.is_match(line) {
                    let msg = line.trim().chars().take(40).collect::<String>();
                    return StatusDetection::new(AgentStatus::Error(msg.clone()))
                        .with_reason(format!("Error pattern matched: {}", msg))
                        .with_pattern("ERROR_PATTERNS");
                }
            }
            return StatusDetection::new(AgentStatus::Error("Error detected".to_string()))
                .with_reason("Error pattern matched in output")
                .with_pattern("ERROR_PATTERNS");
        }
    }

    // 3. Check last 3 non-empty lines for spinners/tool execution
    let last_3_lines: Vec<&str> = lines
        .iter()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(3)
        .cloned()
        .collect();
    let last_3_text = last_3_lines.join("\n");

    if SPINNER_CHARS.is_match(&last_3_text) {
        return StatusDetection::new(AgentStatus::Running)
            .with_reason("Found spinner characters in last 3 lines")
            .with_pattern("SPINNER_CHARS");
    }

    for pattern in TOOL_PATTERNS.iter() {
        if pattern.is_match(&last_3_text) {
            return StatusDetection::new(AgentStatus::Running)
                .with_reason("Found tool execution pattern")
                .with_pattern("TOOL_PATTERNS");
        }
    }

    // 4. Check for prompt character
    let is_at_prompt = lines
        .iter()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(5)
        .any(|line| {
            let trimmed = line.trim().trim_matches('\u{00A0}');
            if trimmed == ">"
                || trimmed == "›"
                || trimmed == "❯"
                || trimmed == "$"
                || trimmed == "%"
            {
                return true;
            }
            if trimmed.len() <= 3
                && (trimmed.starts_with('>')
                    || trimmed.starts_with('›')
                    || trimmed.starts_with('❯')
                    || trimmed.starts_with('$')
                    || trimmed.starts_with('%'))
            {
                return true;
            }
            false
        });

    if is_at_prompt {
        let last_10_lines: Vec<&str> = lines
            .iter()
            .rev()
            .filter(|l| !l.trim().is_empty())
            .take(10)
            .cloned()
            .collect();
        let last_10_text = last_10_lines.join("\n");

        for pattern in COMPLETION_PATTERNS.iter() {
            if pattern.is_match(&last_10_text) {
                return StatusDetection::new(AgentStatus::Completed)
                    .with_reason("At prompt with completion pattern")
                    .with_pattern("COMPLETION_PATTERNS");
            }
        }
        return StatusDetection::new(AgentStatus::Idle).with_reason("At prompt, ready for input");
    }

    // 5. Default: Claude process is running
    StatusDetection::new(AgentStatus::Running)
        .with_reason("Claude process running, no specific indicators")
}

/// Status detection when the foreground is a shell (bash/zsh/etc).
/// Claude has exited — check for errors, otherwise Idle.
fn detect_status_at_shell(output: &str) -> StatusDetection {
    let clean_output = strip_ansi(output);
    let lines: Vec<&str> = clean_output.lines().collect();

    // Check recent lines for error patterns
    let recent_lines: Vec<&str> = lines.iter().rev().take(10).cloned().collect();
    let recent_text = recent_lines.join("\n");

    for pattern in ERROR_PATTERNS.iter() {
        if pattern.is_match(&recent_text) {
            for line in recent_lines.iter() {
                if pattern.is_match(line) {
                    let msg = line.trim().chars().take(40).collect::<String>();
                    return StatusDetection::new(AgentStatus::Error(msg.clone()))
                        .with_reason(format!("Error detected at shell: {}", msg))
                        .with_pattern("ERROR_PATTERNS");
                }
            }
            return StatusDetection::new(AgentStatus::Error("Error detected".to_string()))
                .with_reason("Error pattern matched at shell")
                .with_pattern("ERROR_PATTERNS");
        }
    }

    StatusDetection::new(AgentStatus::Idle)
        .with_reason("Shell process in foreground, no errors detected")
}

/// Agent-aware status detection using process-level ground truth.
/// Routes to the appropriate agent-specific detection function.
pub fn detect_status_for_agent(
    output: &str,
    foreground: ForegroundProcess,
    agent_type: AiAgent,
) -> StatusDetection {
    match agent_type {
        AiAgent::ClaudeCode => detect_status_with_process(output, foreground),
        AiAgent::Opencode => detect_status_opencode(output, foreground),
        AiAgent::Codex => detect_status_codex(output, foreground),
        AiAgent::Gemini => detect_status_gemini(output, foreground),
    }
}

/// Status detection for OpenCode agent.
/// Simple detection: "Permission required" = AwaitingInput, "esc interrupt" = Running, else Idle
fn detect_status_opencode(output: &str, foreground: ForegroundProcess) -> StatusDetection {
    let clean_output = strip_ansi(output);
    let lines: Vec<&str> = clean_output.lines().collect();

    if lines.is_empty() {
        return StatusDetection::new(AgentStatus::Stopped)
            .with_reason("No output captured from tmux pane");
    }

    // Use full output for question/permission detection (these can appear anywhere)
    let full_lower = clean_output.to_lowercase();

    // Also check last 5 non-empty lines for working indicators (bottom of screen where status appears)
    let last_5_lines: Vec<&str> = lines
        .iter()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(5)
        .cloned()
        .collect();
    let last_5_text = last_5_lines.join("\n").to_lowercase();

    // 1. Check for permission panel (highest priority)
    if full_lower.contains("permission required") {
        return StatusDetection::new(AgentStatus::AwaitingInput)
            .with_reason("Found 'permission required' panel")
            .with_pattern("permission_required");
    }

    // 2. Check for plan mode / multi-question panel
    // Look for "Type your own answer" or keyboard hints like "tab" + "select" + "confirm"
    let is_question_panel = full_lower.contains("type your own answer")
        || full_lower.contains("esc dismiss")
        || (full_lower.contains("tab")
            && full_lower.contains("select")
            && full_lower.contains("confirm"))
        || (full_lower.contains("asked") && full_lower.contains("question"));

    if is_question_panel {
        return StatusDetection::new(AgentStatus::AwaitingInput)
            .with_reason("Found question/plan mode panel")
            .with_pattern("question_panel");
    }

    // 3. Check for working indicator ("esc interrupt" without "to" - excludes plan mode hints)
    // Running shows: "esc interrupt" (e.g., "⬝⬝⬝⬝  esc interrupt")
    // Plan mode shows: "esc to interrupt" (hint text, not actual working state)
    if last_5_text.contains("esc interrupt") && !last_5_text.contains("esc to interrupt") {
        return StatusDetection::new(AgentStatus::Running)
            .with_reason("Found 'esc interrupt' (not 'esc to interrupt') in last 5 lines")
            .with_pattern("esc_interrupt");
    }
    // Check for progress animation (multiple consecutive dots)
    if OPENCODE_PROGRESS_PATTERN.is_match(&last_5_text) {
        return StatusDetection::new(AgentStatus::Running)
            .with_reason("Found progress animation (4+ consecutive dots)")
            .with_pattern("OPENCODE_PROGRESS_PATTERN");
    }
    // Check for braille spinner characters
    if OPENCODE_SPINNER_CHARS.is_match(&last_5_text) {
        return StatusDetection::new(AgentStatus::Running)
            .with_reason("Found braille spinner characters")
            .with_pattern("OPENCODE_SPINNER_CHARS");
    }

    // 4. Check for errors
    for pattern in ERROR_PATTERNS.iter() {
        if pattern.is_match(&clean_output) {
            for line in lines.iter().rev().take(15) {
                if pattern.is_match(line) {
                    let msg = line.trim().chars().take(40).collect::<String>();
                    return StatusDetection::new(AgentStatus::Error(msg.clone()))
                        .with_reason(format!("Error pattern matched: {}", msg))
                        .with_pattern("ERROR_PATTERNS");
                }
            }
            return StatusDetection::new(AgentStatus::Error("Error detected".to_string()))
                .with_reason("Error pattern matched in output")
                .with_pattern("ERROR_PATTERNS");
        }
    }

    // 5. Check for completion patterns
    for pattern in COMPLETION_PATTERNS.iter() {
        if pattern.is_match(&clean_output) {
            return StatusDetection::new(AgentStatus::Completed)
                .with_reason("Found completion pattern")
                .with_pattern("COMPLETION_PATTERNS");
        }
    }

    // 6. Check for shell prompt (indicates AI has exited)
    // A shell prompt is typically a short line ending with $ or # at the very end of output
    let last_line = lines.last().map(|l| l.trim()).unwrap_or("");
    let is_shell_prompt = last_line.len() <= 50
        && (last_line.ends_with('$')
            || last_line.ends_with('#')
            || last_line == ">"
            || last_line.starts_with("➜"));

    // 7. Process-based fallback
    // - Shell prompt visible = Stopped (AI has exited)
    // - OpencodeRunning + no working indicators = Idle (AI alive, waiting for input)
    // - Shell process = Stopped (AI has exited, at shell prompt)
    if is_shell_prompt && foreground != ForegroundProcess::OpencodeRunning {
        return StatusDetection::new(AgentStatus::Stopped)
            .with_reason(format!("Shell prompt detected: '{}'", last_line));
    }

    match foreground {
        ForegroundProcess::OpencodeRunning => StatusDetection::new(AgentStatus::Idle)
            .with_reason("Opencode process running, no activity indicators"),
        ForegroundProcess::Shell => {
            StatusDetection::new(AgentStatus::Stopped).with_reason("Shell process in foreground")
        }
        ForegroundProcess::OtherProcess(p) => StatusDetection::new(AgentStatus::Running)
            .with_reason(format!("Subprocess '{}' in foreground", p)),
        ForegroundProcess::Unknown
        | ForegroundProcess::ClaudeRunning
        | ForegroundProcess::CodexRunning
        | ForegroundProcess::GeminiRunning => {
            if clean_output.trim().is_empty() {
                StatusDetection::new(AgentStatus::Stopped)
                    .with_reason("Empty output, unknown process state")
            } else {
                StatusDetection::new(AgentStatus::Idle)
                    .with_reason("Unknown process state, defaulting to idle")
            }
        }
    }
}

/// Pattern for Codex "Working" status line: "Working (Xs • esc to interrupt)"
static CODEX_WORKING_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"•\s*working\s*\(\d+s").unwrap());

/// Status detection for Codex agent.
/// Patterns: "• Working (Xs • esc to interrupt)" = Running, "Question X/X (X unanswered)" = AwaitingInput
fn detect_status_codex(output: &str, foreground: ForegroundProcess) -> StatusDetection {
    let clean_output = strip_ansi(output);
    let lines: Vec<&str> = clean_output.lines().collect();

    if lines.is_empty() {
        return StatusDetection::new(AgentStatus::Stopped)
            .with_reason("No output captured from tmux pane");
    }

    let full_lower = clean_output.to_lowercase();

    // 1. Check for question panel (highest priority)
    let is_question_panel = full_lower.contains("unanswered")
        || full_lower.contains("tab to add notes")
        || (full_lower.contains("navigate") && full_lower.contains("questions"))
        || (full_lower.contains("enter to submit") && full_lower.contains("answer"));

    if is_question_panel {
        return StatusDetection::new(AgentStatus::AwaitingInput)
            .with_reason("Found question panel indicators")
            .with_pattern("codex_question_panel");
    }

    // 2. Check for working indicator: "• Working (Xs"
    if CODEX_WORKING_PATTERN.is_match(&full_lower) {
        return StatusDetection::new(AgentStatus::Running)
            .with_reason("Found 'Working (Xs' indicator")
            .with_pattern("CODEX_WORKING_PATTERN");
    }

    // 3. Check for spinner characters
    let last_5_lines: Vec<&str> = lines.iter().rev().take(5).cloned().collect();
    let last_5_text = last_5_lines.join("\n");
    if SPINNER_CHARS.is_match(&last_5_text) {
        return StatusDetection::new(AgentStatus::Running)
            .with_reason("Found spinner characters in last 5 lines")
            .with_pattern("SPINNER_CHARS");
    }
    if OPENCODE_SPINNER_CHARS.is_match(&last_5_text) {
        return StatusDetection::new(AgentStatus::Running)
            .with_reason("Found braille spinner in last 5 lines")
            .with_pattern("OPENCODE_SPINNER_CHARS");
    }

    // 4. Check for errors
    for pattern in ERROR_PATTERNS.iter() {
        if pattern.is_match(&clean_output) {
            for line in lines.iter().rev().take(15) {
                if pattern.is_match(line) {
                    let msg = line.trim().chars().take(40).collect::<String>();
                    return StatusDetection::new(AgentStatus::Error(msg.clone()))
                        .with_reason(format!("Error pattern matched: {}", msg))
                        .with_pattern("ERROR_PATTERNS");
                }
            }
            return StatusDetection::new(AgentStatus::Error("Error detected".to_string()))
                .with_reason("Error pattern matched in output")
                .with_pattern("ERROR_PATTERNS");
        }
    }

    // 5. Check for prompt character (› or >)
    let is_at_prompt = lines.iter().rev().take(5).any(|line| {
        let trimmed = line.trim();
        trimmed == "›" || trimmed == ">" || trimmed.starts_with("›")
    });

    if is_at_prompt {
        let last_10_lines: Vec<&str> = lines.iter().rev().take(10).cloned().collect();
        let last_10_text = last_10_lines.join("\n");

        for pattern in COMPLETION_PATTERNS.iter() {
            if pattern.is_match(&last_10_text) {
                return StatusDetection::new(AgentStatus::Completed)
                    .with_reason("At prompt with completion pattern")
                    .with_pattern("COMPLETION_PATTERNS");
            }
        }
        return StatusDetection::new(AgentStatus::Idle).with_reason("At prompt, ready for input");
    }

    // 6. Check for shell prompt
    let last_line = lines.last().map(|l| l.trim()).unwrap_or("");
    let is_shell_prompt = last_line.len() <= 50
        && (last_line.ends_with('$')
            || last_line.ends_with('#')
            || last_line == ">"
            || last_line.starts_with("➜"));

    if is_shell_prompt && foreground != ForegroundProcess::CodexRunning {
        return StatusDetection::new(AgentStatus::Stopped)
            .with_reason(format!("Shell prompt detected: '{}'", last_line));
    }

    // 7. Process-based fallback
    match foreground {
        ForegroundProcess::CodexRunning => StatusDetection::new(AgentStatus::Idle)
            .with_reason("Codex process running, no activity indicators"),
        ForegroundProcess::Shell => {
            StatusDetection::new(AgentStatus::Stopped).with_reason("Shell process in foreground")
        }
        ForegroundProcess::OtherProcess(p) => StatusDetection::new(AgentStatus::Running)
            .with_reason(format!("Subprocess '{}' in foreground", p)),
        ForegroundProcess::Unknown
        | ForegroundProcess::ClaudeRunning
        | ForegroundProcess::OpencodeRunning
        | ForegroundProcess::GeminiRunning => {
            if clean_output.trim().is_empty() {
                StatusDetection::new(AgentStatus::Stopped)
                    .with_reason("Empty output, unknown process state")
            } else {
                StatusDetection::new(AgentStatus::Running)
                    .with_reason("Unknown process state, defaulting to running")
            }
        }
    }
}

/// Status detection for Gemini agent.
/// Detection: "Action Required" = AwaitingInput, "esc to cancel" = Running, else Idle
fn detect_status_gemini(output: &str, foreground: ForegroundProcess) -> StatusDetection {
    let clean_output = strip_ansi(output);
    let lines: Vec<&str> = clean_output.lines().collect();

    if lines.is_empty() {
        return StatusDetection::new(AgentStatus::Stopped)
            .with_reason("No output captured from tmux pane");
    }

    let last_5_lines: Vec<&str> = lines.iter().rev().take(5).cloned().collect();
    let last_5_text = last_5_lines.join("\n").to_lowercase();

    // 1. Check for "Action Required" banner (highest priority for AwaitingInput)
    if GEMINI_ACTION_REQUIRED.is_match(&clean_output) {
        return StatusDetection::new(AgentStatus::AwaitingInput)
            .with_reason("Found 'Action Required' banner")
            .with_pattern("GEMINI_ACTION_REQUIRED");
    }

    // 2. Check for "Waiting for confirmation" dialog
    if GEMINI_WAITING_CONFIRMATION.is_match(&clean_output) {
        return StatusDetection::new(AgentStatus::AwaitingInput)
            .with_reason("Found 'Waiting for confirmation' dialog")
            .with_pattern("GEMINI_WAITING_CONFIRMATION");
    }

    // 3. Check for "Answer Questions" panel (Gemini's question dialog)
    if GEMINI_ANSWER_QUESTIONS.is_match(&clean_output) {
        return StatusDetection::new(AgentStatus::AwaitingInput)
            .with_reason("Found 'Answer Questions' panel")
            .with_pattern("GEMINI_ANSWER_QUESTIONS");
    }

    // 4. Check for keyboard hints indicating question panel
    if GEMINI_KEYBOARD_HINTS.is_match(&clean_output) {
        return StatusDetection::new(AgentStatus::AwaitingInput)
            .with_reason("Found keyboard hints in question panel")
            .with_pattern("GEMINI_KEYBOARD_HINTS");
    }

    // 5. Check for running indicators FIRST
    if GEMINI_ESC_CANCEL_TIMER.is_match(&clean_output) {
        return StatusDetection::new(AgentStatus::Running)
            .with_reason("Found 'esc to cancel' timer indicator")
            .with_pattern("GEMINI_ESC_CANCEL_TIMER");
    }

    if GEMINI_DOTS_SPINNER.is_match(&last_5_text) {
        return StatusDetection::new(AgentStatus::Running)
            .with_reason("Found Gemini dots spinner")
            .with_pattern("GEMINI_DOTS_SPINNER");
    }

    if SPINNER_CHARS.is_match(&last_5_text) {
        return StatusDetection::new(AgentStatus::Running)
            .with_reason("Found braille spinner characters")
            .with_pattern("SPINNER_CHARS");
    }

    // 6. Check for numbered questions
    let has_numbered_answers = lines
        .iter()
        .rev()
        .take(20)
        .any(|line| GEMINI_NUMBERED_ANSWERS.is_match(line));

    if !has_numbered_answers {
        for line in lines.iter() {
            if GEMINI_NUMBERED_QUESTIONS.is_match(line) {
                return StatusDetection::new(AgentStatus::AwaitingInput)
                    .with_reason("Found numbered questions awaiting answer")
                    .with_pattern("GEMINI_NUMBERED_QUESTIONS");
            }
        }
    }

    // 7. Check for permission/confirmation prompts
    for pattern in GEMINI_CONFIRMATION_PATTERNS.iter() {
        if pattern.is_match(&last_5_text) {
            return StatusDetection::new(AgentStatus::AwaitingInput)
                .with_reason("Found confirmation prompt")
                .with_pattern("GEMINI_CONFIRMATION_PATTERNS");
        }
    }

    // 8. Check for standard question patterns
    for pattern in QUESTION_PATTERNS.iter() {
        if pattern.is_match(&last_5_text) {
            return StatusDetection::new(AgentStatus::AwaitingInput)
                .with_reason("Found question pattern")
                .with_pattern("QUESTION_PATTERNS");
        }
    }

    // 9. Check for errors
    for pattern in ERROR_PATTERNS.iter() {
        if pattern.is_match(&clean_output) {
            for line in lines.iter().rev().take(15) {
                if pattern.is_match(line) {
                    let msg = line.trim().chars().take(40).collect::<String>();
                    return StatusDetection::new(AgentStatus::Error(msg.clone()))
                        .with_reason(format!("Error pattern matched: {}", msg))
                        .with_pattern("ERROR_PATTERNS");
                }
            }
            return StatusDetection::new(AgentStatus::Error("Error detected".to_string()))
                .with_reason("Error pattern matched in output")
                .with_pattern("ERROR_PATTERNS");
        }
    }

    // 10. Check for prompt character
    let is_at_ai_prompt = lines.iter().rev().take(5).any(|line| {
        let trimmed = line.trim().trim_matches('\u{00A0}');
        if trimmed == ">" || trimmed == "›" || trimmed == "❯" {
            return true;
        }
        if trimmed.starts_with('>') || trimmed.starts_with('›') || trimmed.starts_with('❯') {
            return true;
        }
        false
    });

    if is_at_ai_prompt {
        let last_10_lines: Vec<&str> = lines.iter().rev().take(10).cloned().collect();
        let last_10_text = last_10_lines.join("\n");

        for pattern in COMPLETION_PATTERNS.iter() {
            if pattern.is_match(&last_10_text) {
                return StatusDetection::new(AgentStatus::Completed)
                    .with_reason("At prompt with completion pattern")
                    .with_pattern("COMPLETION_PATTERNS");
            }
        }
        return StatusDetection::new(AgentStatus::Idle).with_reason("At prompt, ready for input");
    }

    // 11. Check for shell prompt
    let last_line = lines.last().map(|l| l.trim()).unwrap_or("");
    let is_shell_prompt = last_line.len() <= 50
        && (last_line.ends_with('$')
            || last_line.ends_with('#')
            || last_line.starts_with('$')
            || last_line.starts_with('#')
            || last_line.starts_with("➜"));

    if is_shell_prompt && foreground != ForegroundProcess::GeminiRunning {
        return StatusDetection::new(AgentStatus::Stopped)
            .with_reason(format!("Shell prompt detected: '{}'", last_line));
    }

    // 12. Process-based fallback
    match foreground {
        ForegroundProcess::GeminiRunning => StatusDetection::new(AgentStatus::Idle)
            .with_reason("Gemini process running, no activity indicators"),
        ForegroundProcess::Shell => {
            StatusDetection::new(AgentStatus::Stopped).with_reason("Shell process in foreground")
        }
        ForegroundProcess::OtherProcess(p) => StatusDetection::new(AgentStatus::Running)
            .with_reason(format!("Subprocess '{}' in foreground", p)),
        ForegroundProcess::Unknown
        | ForegroundProcess::ClaudeRunning
        | ForegroundProcess::OpencodeRunning
        | ForegroundProcess::CodexRunning => {
            if clean_output.trim().is_empty() {
                StatusDetection::new(AgentStatus::Stopped)
                    .with_reason("Empty output, unknown process state")
            } else {
                StatusDetection::new(AgentStatus::Running)
                    .with_reason("Unknown process state, defaulting to running")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opencode_plan_mode_awaiting_input() {
        let output = r#"     → Asked 4 questions
  ┃  5. Type your own answer
  ┃  ⇆ tab  ↑↓ select  enter confirm  esc dismiss
  ┃  • OpenCode 1.2.6"#;
        let status = detect_status_opencode(output, ForegroundProcess::OpencodeRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput, got {:?}",
            status
        );
    }

    #[test]
    fn test_opencode_plan_mode_with_ansi_codes() {
        // Simulate output with ANSI color codes
        let output = "\x1b[36m     → Asked 4 questions\x1b[0m\n  ┃  \x1b[32m5. Type your own answer\x1b[0m\n  ┃  ⇆ tab  ↑↓ select  enter confirm  esc dismiss";
        let status = detect_status_opencode(output, ForegroundProcess::OpencodeRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput with ANSI codes, got {:?}",
            status
        );
    }

    #[test]
    fn test_opencode_idle_with_plan_mode() {
        let output = r#"  ┃  hints.                                                                                                                                           Claude Code status check refinement:
  ┃                                                                                                                                                    tmux outputs analysis plan
  ┃  Looking at the last few non-empty lines:
  ┃  - The system reminder text (plan mode)                                                                                                           Context
  ┃  - ❯ (prompt)                                                                                                                                     78,330 tokens
  ┃  - Separator line                                                                                                                                 38% used
  ┃  - ⏸ plan mode on (shift+tab to cycle) · esc to interrupt <system-reminder>                                                                       $0.00 spent
  ┃
  ┃  The agent is at the prompt (❯) waiting for user input. There's no spinner currently running. The "⏸ plan mode on" is a hint, not an active       LSP
  ┃  spinner.                                                                                                                                         LSPs will activate as files are read
  ┃
  ┃  Actually, wait - there's a spinner ✽ visible: "✽ Julienning…". But looking more carefully, this appears to be historical output - the current    Modified Files
  ┃   state shows the prompt ❯ at the bottom.                                                                                                         src/agent/detector.rs        +261 -24
  ┃
  ┃  The correct status should be Idle because:
  ┃  1. The agent is at the prompt (❯)
  ┃  2. No active spinner in the last few lines
  ┃  3. No question/permission prompt requiring user input
  ┃  4. Just a plan mode hint, which is informational, not an input requirement
  ┃
  ┃  However, there's also consideration for whether plan mode should be a special status. Looking at the existing statuses:
  ┃  - Running
  ┃  - AwaitingInput
  ┃  - Completed
  ┃  - Idle
  ┃  - Error
  ┃  - Stopped
  ┃  - Paused
  ┃
  ┃  There is a Paused status. But plan mode is different from paused - it's a mode where the agent can read but not write. The agent is still
  ┃  active and waiting for input.
  ┃
  ┃  I think Idle is correct here - the agent is at the prompt, ready for input. Plan mode is just a mode, not a state that prevents input.

     Status: Idle

     The agent is at the prompt (❯) with no active spinner, no question requiring input, and no error. Plan mode is just a mode indicator, not an
     input requirement. The agent is ready for user input.

     ▣  Plan · glm-5 · 1m 4s

  ┃
  ┃                                                                                                                                                   ~/.grove/worktrees/0718d9772458e2b2/
  ┃                                                                                                                                                   tidy-up-status-checks
  ┃  Plan  GLM-5 Z.AI Coding Plan                                                                                                                     
  ╹▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
                                                                                                                       tab agents  ctrl+p commands    • OpenCode 1.2.10"#;
        let status = detect_status_opencode(output, ForegroundProcess::OpencodeRunning);
        assert!(
            matches!(status.status, AgentStatus::Idle),
            "Expected Idle with plan mode footer, got {:?}",
            status
        );
    }

    #[test]
    fn test_opencode_plan_mode_hint_not_running() {
        // Plan mode hint with "esc to interrupt" should NOT trigger Running
        // This was causing false positives when checking "esc" && "interrupt" separately
        let output = "  ┃  ⏸ plan mode on (shift+tab to cycle) · esc to interrupt";
        let status = detect_status_opencode(output, ForegroundProcess::OpencodeRunning);
        assert!(
            matches!(status.status, AgentStatus::Idle),
            "Expected Idle, got {:?}",
            status
        );
    }

    #[test]
    fn test_opencode_running_with_dots_spinner() {
        let output = r#"  ┃  Let me create the file with varied paragraphs.

     ~ Preparing write...

     ▣  Build · MiniMax-M2.5

  ┃
  ┃
  ┃                                                                                                                                                   ~/.grove/worktrees/test/
  ┃  Build  MiniMax-M2.5 MiniMax Coding Plan                                                                                                          project
  ╹▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
   ⬝⬝⬝⬝⬝⬝⬝⬝  esc interrupt"#;
        let status = detect_status_opencode(output, ForegroundProcess::OpencodeRunning);
        assert!(
            matches!(status.status, AgentStatus::Running),
            "Expected Running with dots spinner, got {:?}",
            status
        );
    }

    #[test]
    fn test_opencode_running_with_dots_spinner_trailing_newlines() {
        let output = "  ┃  Let me create the file.\n\n     ~ Preparing write...\n\n     ▣  Build · MiniMax-M2.5\n\n  ┃\n\n  ╹▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀\n   ⬝⬝⬝⬝⬝⬝⬝⬝  esc interrupt\n\n\n\n\n";
        let status = detect_status_opencode(output, ForegroundProcess::OpencodeRunning);
        assert!(
            matches!(status.status, AgentStatus::Running),
            "Expected Running with dots spinner and trailing newlines, got {:?}",
            status
        );
    }

    #[test]
    fn test_opencode_checklist_progress() {
        let output = r#"  [✓] Create types.ts with Todo interface
  [✓] Create storage.ts localStorage helpers
  [•] Update index.tsx and styles
  [ ] Test and verify"#;
        let progress = detect_checklist_progress(output, AiAgent::Opencode);
        assert_eq!(
            progress,
            Some((2, 4)),
            "Expected (2, 4), got {:?}",
            progress
        );
    }

    #[test]
    fn test_opencode_side_panel_todos() {
        // Simulates OpenCode UI where side panel is on the right side of lines
        // Todos in side panel should be counted, chat content should be ignored
        let output = r#"  ┃   40   return visible                                                                                                                             [✓] Create src/types.ts
  ┃   41 }                                                                                                                                             [✓] Create src/lib/storage.ts
  ┃                                                                                                                                                   [•] Create src/hooks/useTodos.ts
  ┃  Let me fix these and continue creating components.                                                                                               [ ] Create src/components/Terminal.tsx
  ┃                                                                                                                                                   [ ] Create src/components/TodoItem.tsx"#;
        let progress = detect_checklist_progress(output, AiAgent::Opencode);
        assert_eq!(
            progress,
            Some((2, 5)),
            "Expected (2, 5), got {:?}",
            progress
        );
    }

    #[test]
    fn test_opencode_chat_todos_ignored() {
        // When todos appear in chat (left side of long lines) they should NOT be counted
        // Only side panel todos (rightmost 60 chars) should count
        // Line must be >60 chars with checkbox appearing before the last 60 chars
        let chat_line = "  ┃  I am discussing [✓] Create types.ts in the chat conversation                                                    some regular content here";
        let progress = detect_checklist_progress(chat_line, AiAgent::Opencode);
        assert_eq!(
            progress, None,
            "Chat-only todos should not be counted, got {:?}",
            progress
        );
    }

    #[test]
    fn test_claude_code_checklist_progress() {
        // Claude Code style: todos at line start, various checkbox formats
        let output = r#"  ✓ Create types.ts with Todo interface
  ✓ Create storage.ts localStorage helpers
  ◼ Update index.tsx and styles
  ◻ Test and verify"#;
        let progress = detect_checklist_progress(output, AiAgent::ClaudeCode);
        assert_eq!(
            progress,
            Some((2, 4)),
            "Expected (2, 4) for Claude Code, got {:?}",
            progress
        );
    }

    #[test]
    fn test_claude_code_task_summary() {
        // Claude Code shows authoritative task summary line
        let output = "Some output here\n11 tasks (9 done, 1 in progress, 1 open)\nMore output";
        let progress = detect_checklist_progress(output, AiAgent::ClaudeCode);
        assert_eq!(
            progress,
            Some((9, 11)),
            "Expected (9, 11) from task summary, got {:?}",
            progress
        );
    }

    #[test]
    fn test_claude_code_bracketed_todos() {
        // Claude Code also supports bracketed format at line start
        let output = r#"  [✓] Create types.ts
  [✓] Create storage.ts
  [ ] Test and verify"#;
        let progress = detect_checklist_progress(output, AiAgent::ClaudeCode);
        assert_eq!(
            progress,
            Some((2, 3)),
            "Expected (2, 3) for bracketed todos, got {:?}",
            progress
        );
    }

    #[test]
    fn test_spinner_running() {
        assert!(matches!(
            detect_status("Some output\n⠋ Reading file...").status,
            AgentStatus::Running
        ));
        assert!(matches!(
            detect_status("⠹ Thinking...").status,
            AgentStatus::Running
        ));
    }

    #[test]
    fn test_tool_running() {
        assert!(matches!(
            detect_status("⏺ Read src/main.rs").status,
            AgentStatus::Running
        ));
    }

    #[test]
    fn test_question_awaiting() {
        assert!(matches!(
            detect_status("Allow this? (y/n)").status,
            AgentStatus::AwaitingInput
        ));
        assert!(matches!(
            detect_status("Do you want to proceed? [Y/n]").status,
            AgentStatus::AwaitingInput
        ));
    }

    #[test]
    fn test_prompt_idle() {
        // At the prompt with no questions = idle, ready for new task
        assert!(matches!(
            detect_status("Some text output here.\n>").status,
            AgentStatus::Idle
        ));
        // Also check the chevron prompt
        assert!(matches!(
            detect_status("Some output\n›").status,
            AgentStatus::Idle
        ));
        // Check prompt at end of line
        assert!(matches!(
            detect_status("project >").status,
            AgentStatus::Idle
        ));
        // Claude Code's actual prompt character
        assert!(matches!(
            detect_status("Task complete.\n❯").status,
            AgentStatus::Idle
        ));
        // Claude Code prompt with non-breaking space (U+00A0)
        assert!(matches!(
            detect_status("Task complete.\n❯\u{00A0}").status,
            AgentStatus::Idle
        ));
        // Shell prompt with git info
        assert!(matches!(
            detect_status("Some output\n➜ project git:(main)").status,
            AgentStatus::Idle
        ));
    }

    #[test]
    fn test_error() {
        assert!(matches!(
            detect_status("Error: file not found").status,
            AgentStatus::Error(_)
        ));
        assert!(matches!(
            detect_status("✗ Build failed").status,
            AgentStatus::Error(_)
        ));
    }

    #[test]
    fn test_completion() {
        assert!(matches!(
            detect_status("✓ All tests pass").status,
            AgentStatus::Completed
        ));
    }

    // --- ForegroundProcess tests ---

    #[test]
    fn test_foreground_process_from_command() {
        assert_eq!(
            ForegroundProcess::from_command("node"),
            ForegroundProcess::ClaudeRunning
        );
        assert_eq!(
            ForegroundProcess::from_command("claude"),
            ForegroundProcess::ClaudeRunning
        );
        assert_eq!(
            ForegroundProcess::from_command("npx"),
            ForegroundProcess::ClaudeRunning
        );
        assert_eq!(
            ForegroundProcess::from_command("bash"),
            ForegroundProcess::Shell
        );
        assert_eq!(
            ForegroundProcess::from_command("zsh"),
            ForegroundProcess::Shell
        );
        assert_eq!(
            ForegroundProcess::from_command("fish"),
            ForegroundProcess::Shell
        );
        assert_eq!(
            ForegroundProcess::from_command("cargo"),
            ForegroundProcess::OtherProcess("cargo".to_string())
        );
        assert_eq!(
            ForegroundProcess::from_command("git"),
            ForegroundProcess::OtherProcess("git".to_string())
        );
        assert_eq!(
            ForegroundProcess::from_command(""),
            ForegroundProcess::Unknown
        );
    }

    #[test]
    fn test_foreground_process_with_path() {
        assert_eq!(
            ForegroundProcess::from_command("/usr/bin/node"),
            ForegroundProcess::ClaudeRunning
        );
        assert_eq!(
            ForegroundProcess::from_command("/bin/bash"),
            ForegroundProcess::Shell
        );
    }

    // --- detect_status_with_process tests ---

    #[test]
    fn test_silent_thinking_stays_running() {
        // Claude is running (node foreground) but no output change — should be Running, not Idle
        let output = "Some previous output\nLast line of text";
        let status = detect_status_with_process(output, ForegroundProcess::ClaudeRunning);
        assert!(matches!(status.status, AgentStatus::Running));
    }

    #[test]
    fn test_shell_foreground_immediate_idle() {
        // Claude exited to shell — should be Idle immediately (no 5s lag)
        let output = "Some previous output\n$ ";
        let status = detect_status_with_process(output, ForegroundProcess::Shell);
        assert!(matches!(status.status, AgentStatus::Idle));
    }

    #[test]
    fn test_shell_foreground_with_error() {
        let output = "Error: something went wrong\n$ ";
        let status = detect_status_with_process(output, ForegroundProcess::Shell);
        assert!(matches!(status.status, AgentStatus::Error(_)));
    }

    #[test]
    fn test_other_process_is_running() {
        // Claude spawned cargo — should be Running
        let output = "compiling...\n";
        let status = detect_status_with_process(
            output,
            ForegroundProcess::OtherProcess("cargo".to_string()),
        );
        assert!(matches!(status.status, AgentStatus::Running));
    }

    #[test]
    fn test_unknown_falls_back_to_text_detection() {
        // Unknown foreground — falls back to detect_status()
        let output = "⠋ Reading file...";
        let status = detect_status_with_process(output, ForegroundProcess::Unknown);
        assert!(matches!(status.status, AgentStatus::Running));
    }

    #[test]
    fn test_claude_running_awaiting_input_narrow_window() {
        // Question in last 5 lines → AwaitingInput
        let output = "line1\nline2\nline3\nline4\nAllow this? (y/n)";
        let status = detect_status_with_process(output, ForegroundProcess::ClaudeRunning);
        assert!(matches!(status.status, AgentStatus::AwaitingInput));
    }

    #[test]
    fn test_claude_running_old_question_not_detected() {
        // Question far back (more than 5 lines ago) should NOT trigger AwaitingInput
        let mut lines: Vec<String> = vec!["Allow this? (y/n)".to_string()];
        for i in 0..10 {
            lines.push(format!("working line {}", i));
        }
        let output = lines.join("\n");
        let status = detect_status_with_process(&output, ForegroundProcess::ClaudeRunning);
        // Should NOT be AwaitingInput since the question is >5 lines back
        assert!(!matches!(status.status, AgentStatus::AwaitingInput));
    }

    #[test]
    fn test_claude_running_completed_surfaces() {
        // At prompt with completion pattern → Completed
        let output = "✓ All tests pass\n❯";
        let status = detect_status_with_process(output, ForegroundProcess::ClaudeRunning);
        assert!(matches!(status.status, AgentStatus::Completed));
    }

    #[test]
    fn test_claude_running_blind_thinking_stays_running() {
        // At prompt without completion patterns → Idle
        let output = "Some regular output\n❯";
        let status = detect_status_with_process(output, ForegroundProcess::ClaudeRunning);
        assert!(matches!(status.status, AgentStatus::Idle));
    }

    #[test]
    fn test_claude_running_spinner_running() {
        let output = "Some output\n⠋ Reading file...";
        let status = detect_status_with_process(output, ForegroundProcess::ClaudeRunning);
        assert!(matches!(status.status, AgentStatus::Running));
    }

    #[test]
    fn test_claude_trust_dialog_awaiting_input() {
        let output = r#"──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
 Accessing workspace:

 /home/ziim/.grove/worktrees/d0fd05c68b028185/testclaude

 Quick safety check: Is this a project you created or one you trust? (Like your own code, a well-known open source project, or work from your team). If not, take a moment to review what's
 in this folder first.

 Claude Code'll be able to read, edit, and execute files here.

 Security guide

 ❯ 1. Yes, I trust this folder
   2. No, exit

 Enter to confirm · Esc to cancel"#;
        let status = detect_status_with_process(output, ForegroundProcess::ClaudeRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for trust dialog, got {:?}",
            status
        );
    }

    #[test]
    fn test_claude_trust_dialog_exact_output() {
        let output = "──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n Accessing workspace:\n\n /home/ziim/.grove/worktrees/d0fd05c68b028185/testclaude\n\n Quick safety check: Is this a project you created or one you trust? (Like your own code, a well-known open source project, or work from your team). If not, take a moment to review what's\n in this folder first.\n\n Claude Code'll be able to read, edit, and execute files here.\n\n Security guide\n\n ❯ 1. Yes, I trust this folder\n   2. No, exit\n\n Enter to confirm · Esc to cancel ";
        let status = detect_status_with_process(output, ForegroundProcess::ClaudeRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for exact trust dialog, got {:?}",
            status
        );
    }

    #[test]
    fn test_claude_trust_dialog_with_trailing_newlines() {
        let output = "──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n Accessing workspace:\n\n /home/ziim/.grove/worktrees/d0fd05c68b028185/testclaude\n\n Quick safety check: Is this a project you created or one you trust? (Like your own code, a well-known open source project, or work from your team). If not, take a moment to review what's\n in this folder first.\n\n Claude Code'll be able to read, edit, and execute files here.\n\n Security guide\n\n ❯ 1. Yes, I trust this folder\n   2. No, exit\n\n Enter to confirm · Esc to cancel\n\n\n\n\n\n";
        let status = detect_status_with_process(output, ForegroundProcess::ClaudeRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for trust dialog with trailing newlines, got {:?}",
            status
        );
    }

    #[test]
    fn test_claude_trust_dialog_unknown_process() {
        let output = "──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n Accessing workspace:\n\n /home/ziim/.grove/worktrees/d0fd05c68b028185/testclaude\n\n Quick safety check: Is this a project you created or one you trust? (Like your own code, a well-known open source project, or work from your team). If not, take a moment to review what's\n in this folder first.\n\n Claude Code'll be able to read, edit, and execute files here.\n\n Security guide\n\n ❯ 1. Yes, I trust this folder\n   2. No, exit\n\n Enter to confirm · Esc to cancel ";
        let status = detect_status_with_process(output, ForegroundProcess::Unknown);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for trust dialog with Unknown foreground, got {:?}",
            status
        );
    }

    #[test]
    fn test_claude_trust_dialog_with_ansi() {
        let output = "\x1b[90m────────────────────────────────────────────────\x1b[0m\n\x1b[1m Accessing workspace:\x1b[0m\n\n /home/ziim/test\n\n\x1b[33m Quick safety check:\x1b[0m\n\n \x1b[36m❯ 1. Yes, I trust this folder\x1b[0m\n   2. No, exit\n\n\x1b[2m Enter to confirm · Esc to cancel\x1b[0m ";
        let status = detect_status_with_process(output, ForegroundProcess::ClaudeRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for trust dialog with ANSI codes, got {:?}",
            status
        );
    }

    #[test]
    fn test_other_process_with_input_prompt() {
        let output =
            "❯ 1. Yes, I trust this folder\n   2. No, exit\n\nEnter to confirm · Esc to cancel";
        let status = detect_status_with_process(
            output,
            ForegroundProcess::OtherProcess("cargo".to_string()),
        );
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for OtherProcess with input prompt, got {:?}",
            status
        );
    }

    #[test]
    fn test_other_process_running() {
        let output = "Compiling grove v0.1.0\nRunning tests...";
        let status = detect_status_with_process(
            output,
            ForegroundProcess::OtherProcess("cargo".to_string()),
        );
        assert!(
            matches!(status.status, AgentStatus::Running),
            "Expected Running for OtherProcess without input prompt, got {:?}",
            status
        );
    }

    #[test]
    fn test_other_process_with_error() {
        let output = "Error: build failed\nSome more output";
        let status = detect_status_with_process(
            output,
            ForegroundProcess::OtherProcess("cargo".to_string()),
        );
        assert!(
            matches!(status.status, AgentStatus::Error(_)),
            "Expected Error for OtherProcess with error, got {:?}",
            status
        );
    }

    #[test]
    fn test_claude_numbered_selection_awaiting_input() {
        let output = "❯ 1. First option\n  2. Second option\n  3. Third option";
        let status = detect_status_with_process(output, ForegroundProcess::ClaudeRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for numbered selection, got {:?}",
            status
        );
    }

    #[test]
    fn test_claude_keyboard_hints_awaiting_input() {
        let output = "Some prompt\nEnter to confirm · Esc to cancel";
        let status = detect_status_with_process(output, ForegroundProcess::ClaudeRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for keyboard hints, got {:?}",
            status
        );
    }

    #[test]
    fn test_claude_welcome_screen_idle() {
        let output = r#"╭─── Claude Code v2.1.50 ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│                                                    │ Tips for getting started                                                                                                              │
│                 Welcome back ZiiM!                 │ Run /init to create a CLAUDE.md file with instructions for Claude                                                                     │
│                                                    │ ─────────────────────────────────────────────────────────────────                                                                     │
│                                                    │ Recent activity                                                                                                                       │
│                                                    │ No recent activity                                                                                                                    │
│                          ✻                         │                                                                                                                                       │
│                          |                         │                                                                                                                                       │
│                         ▟█▙                        │                                                                                                                                       │
│                       ▐▛███▜▌                      │                                                                                                                                       │
│                      ▝▜█████▛▘                     │                                                                                                                                       │
│                        ▘▘ ▝▝                       │                                                                                                                                       │
│              Sonnet 4.6 · Claude API               │                                                                                                                                       │
│   ~/.grove/worktrees/d0fd05c68b028185/testclaude   │                                                                                                                                       │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯

──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
❯
──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
  ? for shortcuts  "#;
        let status = detect_status_with_process(output, ForegroundProcess::ClaudeRunning);
        assert!(
            matches!(status.status, AgentStatus::Idle),
            "Expected Idle for welcome screen at prompt, got {:?}",
            status
        );
    }

    // --- Codex tests ---

    #[test]
    fn test_codex_working_status() {
        let output = "Some output\n\n• Working (1s • esc to interrupt)\n";
        let status = detect_status_codex(output, ForegroundProcess::CodexRunning);
        assert!(
            matches!(status.status, AgentStatus::Running),
            "Expected Running, got {:?}",
            status
        );
    }

    #[test]
    fn test_codex_working_longer_duration() {
        let output = "Processing...\n\n• Working (45s • esc to interrupt)\n";
        let status = detect_status_codex(output, ForegroundProcess::CodexRunning);
        assert!(
            matches!(status.status, AgentStatus::Running),
            "Expected Running, got {:?}",
            status
        );
    }

    #[test]
    fn test_codex_question_awaiting_input() {
        let output = "  Question 1/1 (1 unanswered)\n  How should I proceed?\n";
        let status = detect_status_codex(output, ForegroundProcess::CodexRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput, got {:?}",
            status
        );
    }

    #[test]
    fn test_codex_multiple_questions() {
        let output = "  Question 3/5 (2 unanswered)\n  Choose an option:\n";
        let status = detect_status_codex(output, ForegroundProcess::CodexRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput, got {:?}",
            status
        );
    }

    #[test]
    fn test_codex_prompt_idle() {
        let output = "Task complete.\n›";
        let status = detect_status_codex(output, ForegroundProcess::CodexRunning);
        assert!(
            matches!(status.status, AgentStatus::Idle),
            "Expected Idle, got {:?}",
            status
        );
    }

    #[test]
    fn test_codex_error() {
        let output = "Error: something went wrong\n›";
        let status = detect_status_codex(output, ForegroundProcess::CodexRunning);
        assert!(
            matches!(status.status, AgentStatus::Error(_)),
            "Expected Error, got {:?}",
            status
        );
    }

    #[test]
    fn test_codex_foreground_process() {
        assert_eq!(
            ForegroundProcess::from_command_for_agent("codex", AiAgent::Codex),
            ForegroundProcess::CodexRunning
        );
        assert_eq!(
            ForegroundProcess::from_command_for_agent("/usr/bin/codex", AiAgent::Codex),
            ForegroundProcess::CodexRunning
        );
    }

    #[test]
    fn test_codex_completion() {
        let output = "✓ Done.\n›";
        let status = detect_status_codex(output, ForegroundProcess::CodexRunning);
        assert!(
            matches!(status.status, AgentStatus::Completed),
            "Expected Completed, got {:?}",
            status
        );
    }

    #[test]
    fn test_codex_shell_stopped() {
        let output = "Some output\n$ ";
        let status = detect_status_codex(output, ForegroundProcess::Shell);
        assert!(
            matches!(status.status, AgentStatus::Stopped),
            "Expected Stopped, got {:?}",
            status
        );
    }

    #[test]
    fn test_codex_real_question_output() {
        let output = r#"• I'm preparing to ask the user to specify what they mean by document, content, and theme to ensure a clear shared understanding before proceeding.



  Question 1/3 (3 unanswered)
  Where should the 200 paragraphs be written?

  › 1. New file in repo (Recommended)  Create a new text file in the project root.
    2. Specific path                   You'll provide the exact path to write.
    3. Terminal output only            Don't write a file; just print sample in response.
    4. None of the above               Optionally, add details in notes (tab).

  tab to add notes | enter to submit answer | ←/→ to navigate questions | esc to interrupt"#;
        let status = detect_status_codex(output, ForegroundProcess::CodexRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for Question panel, got {:?}",
            status
        );
    }

    #[test]
    fn test_codex_real_working_output() {
        let output = r#"› Lets output a nice 200 paragraphs in a text document


• Working (1s • esc to interrupt)"#;
        let status = detect_status_codex(output, ForegroundProcess::CodexRunning);
        assert!(
            matches!(status.status, AgentStatus::Running),
            "Expected Running for Working indicator, got {:?}",
            status
        );
    }

    // --- Gemini tests ---

    #[test]
    fn test_gemini_action_required_awaiting_input() {
        let output = "Analyzing code...\n\nAction Required\nPlease confirm to proceed";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for Action Required, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_waiting_for_confirmation() {
        let output = "Tool execution\nWaiting for confirmation\n(y/n)";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for Waiting for confirmation, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_esc_cancel_running() {
        let output = "Analyzing...\nWorking on task\n(esc to cancel, 15s)";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status.status, AgentStatus::Running),
            "Expected Running for esc to cancel, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_dots_spinner_running() {
        let output = "Processing...\n⠁ \nThinking";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status.status, AgentStatus::Running),
            "Expected Running for dots spinner, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_braille_spinner_running() {
        let output = "Processing...\n⠋ Working...";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status.status, AgentStatus::Running),
            "Expected Running for braille spinner, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_permission_prompt_awaiting_input() {
        let output = "Tool: Bash\nAllow this? [Y/n]";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for permission prompt, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_completion() {
        let output = "✓ Task completed successfully\n>";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status.status, AgentStatus::Completed),
            "Expected Completed, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_error() {
        let output = "Error: file not found\n>";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status.status, AgentStatus::Error(_)),
            "Expected Error, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_shell_stopped() {
        let output = "Session ended\n$ ";
        let status = detect_status_gemini(output, ForegroundProcess::Shell);
        assert!(
            matches!(status.status, AgentStatus::Stopped),
            "Expected Stopped when at shell, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_running_idle() {
        let output = "Ready for input\n❯";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status.status, AgentStatus::Idle),
            "Expected Idle when Gemini running at prompt, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_foreground_process_detection() {
        assert_eq!(
            ForegroundProcess::from_command_for_agent("node", AiAgent::Gemini),
            ForegroundProcess::GeminiRunning
        );
        assert_eq!(
            ForegroundProcess::from_command_for_agent("gemini", AiAgent::Gemini),
            ForegroundProcess::GeminiRunning
        );
    }

    #[test]
    fn test_gemini_is_agent_running() {
        assert!(ForegroundProcess::GeminiRunning.is_agent_running());
    }

    #[test]
    fn test_gemini_answer_questions_awaiting_input() {
        let output = "╭────────────────────────────────────────────────────╮\n│ Answer Questions                                   │\n╰────────────────────────────────────────────────────╯";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for Answer Questions panel, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_keyboard_hints_awaiting_input() {
        let output = "Enter to select · ←/→ to switch questions · Esc to cancel";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for keyboard hints, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_idle_at_prompt() {
        // At idle prompt with input bar - should be Idle, not Running
        let output = "Type your message\n▀▀▀▀▀▀▀▀▀▀▀▀▀▀\n>   Type your message or @path/to/file";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status.status, AgentStatus::Idle),
            "Expected Idle at Gemini prompt, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_quiet_state_is_idle() {
        // No spinner, no timer, no questions - agent is in quiet/idle state
        // This happens when agent is waiting but not actively working
        let output = r#"Logged in with Google: user@example.com /auth
Plan: Gemini Code Assist for individuals

 > Okay, lets create a plan to add 200 paragraphs
▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
 ~/.grove/worktrees/test, (test,*)                           /model Auto (Gemini 3)"#;
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status.status, AgentStatus::Idle),
            "Expected Idle in quiet state (no spinner/timer), got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_questions_answered_then_running() {
        // User has answered the numbered questions, agent is now running
        // Should be Running, NOT AwaitingInput (old questions in history should be ignored)
        let output = r#"   1. Target File: Should I create a new document?
   2. Paragraph Content: Do you want specific text?
   3. Placement: Should the paragraphs be added to the beginning?
   4. Formatting: Is there a specific format required?

 > 1. New doc
   2. Lorem ipsum
   3. Don't care.
   4. Markdown

 ⠴ Generating witty retort… (esc to cancel, 15s)"#;
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status.status, AgentStatus::Running),
            "Expected Running when user has answered and spinner/timer is present, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_old_questions_with_timer_is_running() {
        // Old questions are visible in scrollback, but timer/spinner shows agent is actively running
        // User's answers have scrolled off - should still detect as Running
        let output = r#"   1. Target File: Should I create a new document?
   2. Paragraph Content: Do you want specific text?

 ...lots of tool output...

 ⠇ Why do Java developers wear glasses? Because they don't C#. (esc to cancel, 12s)"#;
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status.status, AgentStatus::Running),
            "Expected Running when timer is present, even with old questions in history, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_real_answer_questions_panel() {
        // Full output from real Gemini CLI with Answer Questions panel
        let output = r#" ███            █████████  ██████████ ██████   ██████ █████ ██████   █████ █████
░░░███         ███░░░░░███░░███░░░░░█░░██████ ██████ ░░███ ░░██████ ░░███ ░░███
Logged in with Google: alextede8899@gmail.com /auth
Plan: Gemini Code Assist for individuals
Tips for getting started:
1. Ask questions, edit files, or run commands.
2. Be specific for the best results.
3. /help for more information.

▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
 > Write me a nice two hundred paragraph text document
▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄

ℹ No background shells are currently active.
╭────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Answer Questions                                                                                                                                                                           │
│                                                                                                                                                                                            │
│ ← □ Topic │ □ Filename │ ≡ Review →                                                                                                                                                        │
│                                                                                                                                                                                            │
│ What should be the topic or content of the 200 paragraphs?                                                                                                                                 │
│                                                                                                                                                                                            │
│ ▲                                                                                                                                                                                          │
│ ● 1.  Lorem Ipsum                                                                                                                                                                          │
│       Standard placeholder text                                                                                                                                                            │
│   2.  Story                                                                                                                                                                                │
│       A creative fictional story                                                                                                                                                           │
│ ▼                                                                                                                                                                                          │
│                                                                                                                                                                                            │
│ Enter to select · ←/→ to switch questions · Esc to cancel                                                                                                                                  │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯

──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
 shift+tab to accept edits                                                                                                                                                   1 GEMINI.md file
 ~/dev/todo-testapp/.worktrees/gemini (gemini*)                                                 no sandbox (see /docs)                                                 /model Auto (Gemini 3)"#;
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for Answer Questions panel, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_real_answer_questions_with_unknown_process() {
        // Same output but with Unknown foreground - should still detect AwaitingInput
        let output = r#"Answer Questions
│
│ Enter to select · ←/→ to switch questions · Esc to cancel"#;
        let status = detect_status_gemini(output, ForegroundProcess::Unknown);
        assert!(
            matches!(status.status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput with Unknown foreground, got {:?}",
            status
        );
    }
}
