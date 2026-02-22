use regex::Regex;
use std::sync::LazyLock;

use super::AgentStatus;
use crate::app::config::AiAgent;
use crate::gitlab::{MergeRequestStatus, PipelineStatus};

/// Classification of the foreground process in the tmux pane.
/// Used as ground truth for status detection.
#[derive(Debug, Clone, PartialEq)]
pub enum ForegroundProcess {
    /// Claude Code is alive (node, claude, npx)
    ClaudeRunning,
    /// Opencode is alive (node, opencode, npx)
    OpencodeRunning,
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
                AiAgent::Gemini => ForegroundProcess::GeminiRunning,
                AiAgent::Codex => ForegroundProcess::OtherProcess(binary.to_string()),
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
pub fn detect_status(output: &str) -> AgentStatus {
    let clean_output = strip_ansi(output);
    let lines: Vec<&str> = clean_output.lines().collect();

    if lines.is_empty() {
        return AgentStatus::Stopped;
    }

    // Get recent lines for analysis (last 15 lines should capture current state)
    let recent_lines: Vec<&str> = lines.iter().rev().take(15).cloned().collect();
    let recent_text = recent_lines.join("\n");

    // Get the last few lines (where spinners and prompts appear)
    let _last_line = lines.last().copied().unwrap_or("");
    let last_3_lines: Vec<&str> = lines.iter().rev().take(3).cloned().collect();
    let last_3_text = last_3_lines.join("\n");

    // 1. CHECK FOR QUESTIONS/PERMISSION PROMPTS (highest priority)
    // These indicate Claude needs user input for a specific question
    for pattern in QUESTION_PATTERNS.iter() {
        if pattern.is_match(&recent_text) {
            return AgentStatus::AwaitingInput;
        }
    }

    // 2. CHECK FOR RUNNING (before prompt check!)
    // If there are spinners in the last few lines, Claude is actively working
    if SPINNER_CHARS.is_match(&last_3_text) {
        return AgentStatus::Running;
    }

    // Tool execution patterns (in last 3 lines)
    for pattern in TOOL_PATTERNS.iter() {
        if pattern.is_match(&last_3_text) {
            return AgentStatus::Running;
        }
    }

    // 3. CHECK IF AT PROMPT (only after confirming no spinners!)
    // Claude Code uses "❯" as its prompt (often followed by non-breaking space U+00A0)
    // Shells use ">", "›", "➜", "$", "%"
    let is_at_prompt = lines.iter().rev().take(5).any(|line| {
        // Strip both regular whitespace and non-breaking spaces
        let trimmed = line.trim().trim_matches('\u{00A0}');
        // Exact prompt characters (after stripping NBSP)
        if trimmed == ">" || trimmed == "›" || trimmed == "❯" || trimmed == "$" || trimmed == "%"
        {
            return true;
        }
        // Short lines starting with prompt char (like "❯ " or "> ")
        if trimmed.len() <= 3
            && (trimmed.starts_with('>')
                || trimmed.starts_with('›')
                || trimmed.starts_with('❯')
                || trimmed.starts_with('$')
                || trimmed.starts_with('%'))
        {
            return true;
        }
        // Shell prompts with git info like "➜ project git:(branch)"
        if trimmed.starts_with("➜") {
            return true;
        }
        false
    });

    if is_at_prompt {
        // At the prompt with no spinners = idle, ready for input
        return AgentStatus::Idle;
    }

    // 4. CHECK FOR ERRORS
    for pattern in ERROR_PATTERNS.iter() {
        if pattern.is_match(&recent_text) {
            // Try to extract the error message
            for line in recent_lines.iter() {
                if pattern.is_match(line) {
                    let msg = line.trim().chars().take(40).collect::<String>();
                    return AgentStatus::Error(msg);
                }
            }
            return AgentStatus::Error("Error detected".to_string());
        }
    }

    // 5. CHECK FOR COMPLETION
    for pattern in COMPLETION_PATTERNS.iter() {
        if pattern.is_match(&recent_text) {
            return AgentStatus::Completed;
        }
    }

    // 6. DEFAULT
    // If there's output but we can't determine state, assume idle
    if clean_output.trim().is_empty() {
        AgentStatus::Stopped
    } else {
        // Has output, no clear indicators - probably idle at prompt
        AgentStatus::Idle
    }
}

/// Detect agent status using process-level ground truth combined with text analysis.
///
/// This is more accurate than `detect_status()` alone because it uses the tmux
/// foreground process as definitive signal for whether Claude is running.
pub fn detect_status_with_process(output: &str, foreground: ForegroundProcess) -> AgentStatus {
    match foreground {
        ForegroundProcess::ClaudeRunning => detect_status_claude_running(output),
        ForegroundProcess::OpencodeRunning => detect_status_opencode(output, foreground),
        ForegroundProcess::GeminiRunning => detect_status_gemini(output, foreground),
        ForegroundProcess::Shell => detect_status_at_shell(output),
        ForegroundProcess::OtherProcess(_) => AgentStatus::Running,
        ForegroundProcess::Unknown => detect_status(output),
    }
}

/// Status detection when we know Claude (node/npx) is the foreground process.
/// Default is Running (fixes false Idle during silent thinking).
fn detect_status_claude_running(output: &str) -> AgentStatus {
    let clean_output = strip_ansi(output);
    let lines: Vec<&str> = clean_output.lines().collect();

    if lines.is_empty() {
        return AgentStatus::Running;
    }

    // Narrow window: last 5 lines for question patterns (reduces false positives)
    let last_5_lines: Vec<&str> = lines.iter().rev().take(5).cloned().collect();
    let last_5_text = last_5_lines.join("\n");

    // 1. Check for questions/permission prompts (highest priority)
    for pattern in QUESTION_PATTERNS.iter() {
        if pattern.is_match(&last_5_text) {
            return AgentStatus::AwaitingInput;
        }
    }

    // 2. Check for errors in last 5 lines
    for pattern in ERROR_PATTERNS.iter() {
        if pattern.is_match(&last_5_text) {
            for line in last_5_lines.iter() {
                if pattern.is_match(line) {
                    let msg = line.trim().chars().take(40).collect::<String>();
                    return AgentStatus::Error(msg);
                }
            }
            return AgentStatus::Error("Error detected".to_string());
        }
    }

    // 3. Check last 3 lines for spinners/tool execution → Running
    let last_3_lines: Vec<&str> = lines.iter().rev().take(3).cloned().collect();
    let last_3_text = last_3_lines.join("\n");

    if SPINNER_CHARS.is_match(&last_3_text) {
        return AgentStatus::Running;
    }

    for pattern in TOOL_PATTERNS.iter() {
        if pattern.is_match(&last_3_text) {
            return AgentStatus::Running;
        }
    }

    // 4. Check for prompt character → Completed or Idle
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
        false
    });

    if is_at_prompt {
        // Check last 10 lines for completion patterns
        let last_10_lines: Vec<&str> = lines.iter().rev().take(10).cloned().collect();
        let last_10_text = last_10_lines.join("\n");

        for pattern in COMPLETION_PATTERNS.iter() {
            if pattern.is_match(&last_10_text) {
                return AgentStatus::Completed;
            }
        }
        return AgentStatus::Idle;
    }

    // 5. Default: Claude process is running → Running
    // This fixes false Idle during silent thinking (no output change)
    AgentStatus::Running
}

/// Status detection when the foreground is a shell (bash/zsh/etc).
/// Claude has exited — check for errors, otherwise Idle.
fn detect_status_at_shell(output: &str) -> AgentStatus {
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
                    return AgentStatus::Error(msg);
                }
            }
            return AgentStatus::Error("Error detected".to_string());
        }
    }

    AgentStatus::Idle
}

/// Agent-aware status detection using process-level ground truth.
/// Routes to the appropriate agent-specific detection function.
pub fn detect_status_for_agent(
    output: &str,
    foreground: ForegroundProcess,
    agent_type: AiAgent,
) -> AgentStatus {
    match agent_type {
        AiAgent::ClaudeCode => detect_status_with_process(output, foreground),
        AiAgent::Opencode => detect_status_opencode(output, foreground),
        AiAgent::Gemini => detect_status_gemini(output, foreground),
        AiAgent::Codex => detect_status_with_process(output, foreground),
    }
}

/// Status detection for OpenCode agent.
/// Simple detection: "Permission required" = AwaitingInput, "esc interrupt" = Running, else Idle
fn detect_status_opencode(output: &str, foreground: ForegroundProcess) -> AgentStatus {
    let clean_output = strip_ansi(output);
    let lines: Vec<&str> = clean_output.lines().collect();

    if lines.is_empty() {
        return AgentStatus::Stopped;
    }

    // Use full output for question/permission detection (these can appear anywhere)
    let full_lower = clean_output.to_lowercase();

    // Also check last 5 lines for working indicators (bottom of screen where status appears)
    let last_5_lines: Vec<&str> = lines.iter().rev().take(5).cloned().collect();
    let last_5_text = last_5_lines.join("\n").to_lowercase();

    // 1. Check for permission panel (highest priority)
    if full_lower.contains("permission required") {
        return AgentStatus::AwaitingInput;
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
        return AgentStatus::AwaitingInput;
    }

    // 3. Check for working indicator ("esc interrupt" or progress animation at bottom)
    if last_5_text.contains("esc") && last_5_text.contains("interrupt") {
        return AgentStatus::Running;
    }
    // Check for progress animation (multiple consecutive dots)
    if OPENCODE_PROGRESS_PATTERN.is_match(&last_5_text) {
        return AgentStatus::Running;
    }
    // Check for braille spinner characters
    if OPENCODE_SPINNER_CHARS.is_match(&last_5_text) {
        return AgentStatus::Running;
    }

    // 4. Check for errors
    for pattern in ERROR_PATTERNS.iter() {
        if pattern.is_match(&clean_output) {
            for line in lines.iter().rev().take(15) {
                if pattern.is_match(line) {
                    let msg = line.trim().chars().take(40).collect::<String>();
                    return AgentStatus::Error(msg);
                }
            }
            return AgentStatus::Error("Error detected".to_string());
        }
    }

    // 5. Check for completion patterns
    for pattern in COMPLETION_PATTERNS.iter() {
        if pattern.is_match(&clean_output) {
            return AgentStatus::Completed;
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
        return AgentStatus::Stopped;
    }

    match foreground {
        ForegroundProcess::OpencodeRunning => AgentStatus::Idle,
        ForegroundProcess::Shell => AgentStatus::Stopped,
        ForegroundProcess::OtherProcess(_) => AgentStatus::Running,
        ForegroundProcess::Unknown
        | ForegroundProcess::ClaudeRunning
        | ForegroundProcess::GeminiRunning => {
            if clean_output.trim().is_empty() {
                AgentStatus::Stopped
            } else {
                AgentStatus::Idle
            }
        }
    }
}

/// Status detection for Gemini agent.
/// Detection: "Action Required" = AwaitingInput, "esc to cancel" = Running, else Idle
fn detect_status_gemini(output: &str, foreground: ForegroundProcess) -> AgentStatus {
    let clean_output = strip_ansi(output);
    let lines: Vec<&str> = clean_output.lines().collect();

    if lines.is_empty() {
        return AgentStatus::Stopped;
    }

    let last_5_lines: Vec<&str> = lines.iter().rev().take(5).cloned().collect();
    let last_5_text = last_5_lines.join("\n").to_lowercase();

    // 1. Check for "Action Required" banner (highest priority for AwaitingInput)
    if GEMINI_ACTION_REQUIRED.is_match(&clean_output) {
        return AgentStatus::AwaitingInput;
    }

    // 2. Check for "Waiting for confirmation" dialog
    if GEMINI_WAITING_CONFIRMATION.is_match(&clean_output) {
        return AgentStatus::AwaitingInput;
    }

    // 3. Check for "Answer Questions" panel (Gemini's question dialog)
    if GEMINI_ANSWER_QUESTIONS.is_match(&clean_output) {
        return AgentStatus::AwaitingInput;
    }

    // 4. Check for keyboard hints indicating question panel
    if GEMINI_KEYBOARD_HINTS.is_match(&clean_output) {
        return AgentStatus::AwaitingInput;
    }

    // 5. Check for permission/confirmation prompts
    for pattern in GEMINI_CONFIRMATION_PATTERNS.iter() {
        if pattern.is_match(&last_5_text) {
            return AgentStatus::AwaitingInput;
        }
    }

    // 6. Check for standard question patterns (y/n, [Y/n], etc.)
    for pattern in QUESTION_PATTERNS.iter() {
        if pattern.is_match(&last_5_text) {
            return AgentStatus::AwaitingInput;
        }
    }

    // 7. Check for running indicator (timer format only, not keyboard hints)
    if GEMINI_ESC_CANCEL_TIMER.is_match(&clean_output) {
        return AgentStatus::Running;
    }

    // Check for Gemini dots spinner
    if GEMINI_DOTS_SPINNER.is_match(&last_5_text) {
        return AgentStatus::Running;
    }

    // Check for braille spinner characters (shared with other tools)
    if SPINNER_CHARS.is_match(&last_5_text) {
        return AgentStatus::Running;
    }

    // 8. Check for errors
    for pattern in ERROR_PATTERNS.iter() {
        if pattern.is_match(&clean_output) {
            for line in lines.iter().rev().take(15) {
                if pattern.is_match(line) {
                    let msg = line.trim().chars().take(40).collect::<String>();
                    return AgentStatus::Error(msg);
                }
            }
            return AgentStatus::Error("Error detected".to_string());
        }
    }

    // 9. Check for completion patterns
    for pattern in COMPLETION_PATTERNS.iter() {
        if pattern.is_match(&clean_output) {
            return AgentStatus::Completed;
        }
    }

    // 10. Check for shell prompt (indicates AI has exited)
    let last_line = lines.last().map(|l| l.trim()).unwrap_or("");
    let is_shell_prompt = last_line.len() <= 50
        && (last_line.ends_with('$')
            || last_line.ends_with('#')
            || last_line == ">"
            || last_line.starts_with("➜"));

    if is_shell_prompt && foreground != ForegroundProcess::GeminiRunning {
        return AgentStatus::Stopped;
    }

    // 11. Process-based fallback
    match foreground {
        ForegroundProcess::GeminiRunning => AgentStatus::Idle,
        ForegroundProcess::Shell => AgentStatus::Stopped,
        ForegroundProcess::OtherProcess(_) => AgentStatus::Running,
        ForegroundProcess::Unknown
        | ForegroundProcess::ClaudeRunning
        | ForegroundProcess::OpencodeRunning => {
            if clean_output.trim().is_empty() {
                AgentStatus::Stopped
            } else {
                AgentStatus::Idle
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
            matches!(status, AgentStatus::AwaitingInput),
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
            matches!(status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput with ANSI codes, got {:?}",
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
            detect_status("Some output\n⠋ Reading file..."),
            AgentStatus::Running
        ));
        assert!(matches!(
            detect_status("⠹ Thinking..."),
            AgentStatus::Running
        ));
    }

    #[test]
    fn test_tool_running() {
        assert!(matches!(
            detect_status("⏺ Read src/main.rs"),
            AgentStatus::Running
        ));
    }

    #[test]
    fn test_question_awaiting() {
        assert!(matches!(
            detect_status("Allow this? (y/n)"),
            AgentStatus::AwaitingInput
        ));
        assert!(matches!(
            detect_status("Do you want to proceed? [Y/n]"),
            AgentStatus::AwaitingInput
        ));
    }

    #[test]
    fn test_prompt_idle() {
        // At the prompt with no questions = idle, ready for new task
        assert!(matches!(
            detect_status("Some text output here.\n>"),
            AgentStatus::Idle
        ));
        // Also check the chevron prompt
        assert!(matches!(detect_status("Some output\n›"), AgentStatus::Idle));
        // Check prompt at end of line
        assert!(matches!(detect_status("project >"), AgentStatus::Idle));
        // Claude Code's actual prompt character
        assert!(matches!(
            detect_status("Task complete.\n❯"),
            AgentStatus::Idle
        ));
        // Claude Code prompt with non-breaking space (U+00A0)
        assert!(matches!(
            detect_status("Task complete.\n❯\u{00A0}"),
            AgentStatus::Idle
        ));
        // Shell prompt with git info
        assert!(matches!(
            detect_status("Some output\n➜ project git:(main)"),
            AgentStatus::Idle
        ));
    }

    #[test]
    fn test_error() {
        assert!(matches!(
            detect_status("Error: file not found"),
            AgentStatus::Error(_)
        ));
        assert!(matches!(
            detect_status("✗ Build failed"),
            AgentStatus::Error(_)
        ));
    }

    #[test]
    fn test_completion() {
        assert!(matches!(
            detect_status("✓ All tests pass"),
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
        assert!(matches!(status, AgentStatus::Running));
    }

    #[test]
    fn test_shell_foreground_immediate_idle() {
        // Claude exited to shell — should be Idle immediately (no 5s lag)
        let output = "Some previous output\n$ ";
        let status = detect_status_with_process(output, ForegroundProcess::Shell);
        assert!(matches!(status, AgentStatus::Idle));
    }

    #[test]
    fn test_shell_foreground_with_error() {
        let output = "Error: something went wrong\n$ ";
        let status = detect_status_with_process(output, ForegroundProcess::Shell);
        assert!(matches!(status, AgentStatus::Error(_)));
    }

    #[test]
    fn test_other_process_is_running() {
        // Claude spawned cargo — should be Running
        let output = "compiling...\n";
        let status = detect_status_with_process(
            output,
            ForegroundProcess::OtherProcess("cargo".to_string()),
        );
        assert!(matches!(status, AgentStatus::Running));
    }

    #[test]
    fn test_unknown_falls_back_to_text_detection() {
        // Unknown foreground — falls back to detect_status()
        let output = "⠋ Reading file...";
        let status = detect_status_with_process(output, ForegroundProcess::Unknown);
        assert!(matches!(status, AgentStatus::Running));
    }

    #[test]
    fn test_claude_running_awaiting_input_narrow_window() {
        // Question in last 5 lines → AwaitingInput
        let output = "line1\nline2\nline3\nline4\nAllow this? (y/n)";
        let status = detect_status_with_process(output, ForegroundProcess::ClaudeRunning);
        assert!(matches!(status, AgentStatus::AwaitingInput));
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
        assert!(!matches!(status, AgentStatus::AwaitingInput));
    }

    #[test]
    fn test_claude_running_completed_surfaces() {
        // At prompt with completion pattern → Completed
        let output = "✓ All tests pass\n❯";
        let status = detect_status_with_process(output, ForegroundProcess::ClaudeRunning);
        assert!(matches!(status, AgentStatus::Completed));
    }

    #[test]
    fn test_claude_running_prompt_idle() {
        // At prompt without completion patterns → Idle
        let output = "Some regular output\n❯";
        let status = detect_status_with_process(output, ForegroundProcess::ClaudeRunning);
        assert!(matches!(status, AgentStatus::Idle));
    }

    #[test]
    fn test_claude_running_spinner_running() {
        let output = "Some output\n⠋ Reading file...";
        let status = detect_status_with_process(output, ForegroundProcess::ClaudeRunning);
        assert!(matches!(status, AgentStatus::Running));
    }

    // --- Gemini tests ---

    #[test]
    fn test_gemini_action_required_awaiting_input() {
        let output = "Analyzing code...\n\nAction Required\nPlease confirm to proceed";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for Action Required, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_waiting_for_confirmation() {
        let output = "Tool execution\nWaiting for confirmation\n(y/n)";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for Waiting for confirmation, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_esc_cancel_running() {
        let output = "Analyzing...\nWorking on task\n(esc to cancel, 15s)";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status, AgentStatus::Running),
            "Expected Running for esc to cancel, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_dots_spinner_running() {
        let output = "Processing...\n⠁ \nThinking";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status, AgentStatus::Running),
            "Expected Running for dots spinner, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_braille_spinner_running() {
        let output = "Processing...\n⠋ Working...";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status, AgentStatus::Running),
            "Expected Running for braille spinner, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_permission_prompt_awaiting_input() {
        let output = "Tool: Bash\nAllow this? [Y/n]";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for permission prompt, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_completion() {
        let output = "✓ Task completed successfully\n>";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status, AgentStatus::Completed),
            "Expected Completed, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_error() {
        let output = "Error: file not found\n>";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status, AgentStatus::Error(_)),
            "Expected Error, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_shell_stopped() {
        let output = "Session ended\n$ ";
        let status = detect_status_gemini(output, ForegroundProcess::Shell);
        assert!(
            matches!(status, AgentStatus::Stopped),
            "Expected Stopped when at shell, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_running_idle() {
        let output = "Ready for input\n❯";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status, AgentStatus::Idle),
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
            matches!(status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput for Answer Questions panel, got {:?}",
            status
        );
    }

    #[test]
    fn test_gemini_keyboard_hints_awaiting_input() {
        let output = "Enter to select · ←/→ to switch questions · Esc to cancel";
        let status = detect_status_gemini(output, ForegroundProcess::GeminiRunning);
        assert!(
            matches!(status, AgentStatus::AwaitingInput),
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
            matches!(status, AgentStatus::Idle),
            "Expected Idle at Gemini prompt, got {:?}",
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

▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
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
            matches!(status, AgentStatus::AwaitingInput),
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
            matches!(status, AgentStatus::AwaitingInput),
            "Expected AwaitingInput with Unknown foreground, got {:?}",
            status
        );
    }
}
