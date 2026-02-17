use regex::Regex;
use std::sync::LazyLock;

use super::AgentStatus;
use crate::gitlab::{MergeRequestStatus, PipelineStatus};

/// Classification of the foreground process in the tmux pane.
/// Used as ground truth for status detection.
#[derive(Debug, Clone, PartialEq)]
pub enum ForegroundProcess {
    /// Claude Code is alive (node, claude, npx)
    ClaudeRunning,
    /// At a shell prompt (bash, zsh, sh, fish, dash)
    Shell,
    /// Claude spawned a subprocess (cargo, git, python, etc.)
    OtherProcess(String),
    /// Could not determine (tmux error or unavailable)
    Unknown,
}

impl ForegroundProcess {
    /// Classify a process command name into a `ForegroundProcess` variant.
    pub fn from_command(cmd: &str) -> Self {
        let cmd_lower = cmd.to_lowercase();
        // Extract just the binary name (strip path if present)
        let binary = cmd_lower.rsplit('/').next().unwrap_or(&cmd_lower);

        match binary {
            "node" | "claude" | "npx" => ForegroundProcess::ClaudeRunning,
            "bash" | "zsh" | "sh" | "fish" | "dash" => ForegroundProcess::Shell,
            "" => ForegroundProcess::Unknown,
            other => ForegroundProcess::OtherProcess(other.to_string()),
        }
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

/// Detect checklist progress from Claude Code output.
/// Returns (completed, total) if a checklist is found.
/// Checklist items:
/// - ■ ▪ ● (filled shapes) = in progress (counts as incomplete)
/// - □ ☐ ○ (empty shapes) = not started (incomplete)
/// - ✓ ✔ ☑ (checkmarks) = completed
pub fn detect_checklist_progress(output: &str) -> Option<(u32, u32)> {
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

    // Fallback: count individual checklist items
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

        // Look for checklist item patterns at the start of lines (with possible tree characters)
        // Tree chars: │ ├ └ ─ followed by space, then the checkbox
        let check_part = trimmed
            .trim_start_matches(|c| c == '│' || c == '├' || c == '└' || c == '─' || c == ' ');

        // Check for completed items (checkmarks) - various Unicode checkmarks
        // ✓ U+2713, ✔ U+2714, ☑ U+2611
        if check_part.starts_with('✓')
            || check_part.starts_with('✔')
            || check_part.starts_with('☑')
            || check_part.starts_with('✅')
        {
            completed += 1;
            total += 1;
        }
        // Check for in-progress items (filled shapes)
        // ◼ U+25FC (Claude Code uses this), ■ U+25A0, ▪ U+25AA
        else if check_part.starts_with('◼')
            || check_part.starts_with('■')
            || check_part.starts_with('▪')
            || check_part.starts_with('●')
        {
            total += 1;
        }
        // Check for not-started items (empty shapes)
        // ◻ U+25FB (Claude Code uses this), □ U+25A1, ☐ U+2610
        else if check_part.starts_with('◻')
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
    let last_line = lines.last().map(|s| *s).unwrap_or("");
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
