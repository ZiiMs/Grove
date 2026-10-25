//! The tools the agent may call, and their sandbox.
//!
//! Every tool runs locally, in the session's working directory. A path is resolved against that
//! directory and anything that escapes it (`..`, an absolute path elsewhere, a symlink pointing out)
//! is refused: the agent works on the project the user opened and nothing else.
//!
//! A link that points outside the project is refused even when its own path looks relative.\n//!\n//! Three tools change the machine — `write_file`, `edit_file` and `run_command` — and those need the
//! user's approval. The read-only ones do not, because asking about every `read_file` makes the
//! approval cards noise the user learns to click through.

use super::wire::MsgBlock;
use serde::Serialize;
use std::path::{Component, Path, PathBuf};

/// One tool as the model sees it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: serde_json::Value,
}

/// The complete tool set. One list, so all three vendors are offered the same capabilities and a
/// test can assert every tool is also classified for approval.
pub fn specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "read_file",
            description: "Read a UTF-8 text file from the project. Returns the whole file, truncated to 200000 characters.",
            input_schema: object(&[("path", "Repository-relative path to the file to read.")]),
        },
        ToolSpec {
            name: "list_dir",
            description: "List the entries of one directory in the project.",
            input_schema: object(&[("path", "Repository-relative directory. Use \".\" for the project root.")]),
        },
        ToolSpec {
            name: "grep",
            description: "Search the project's text files for a literal substring. Returns matching lines with file and line number, capped at 200 matches.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Literal text to search for." },
                    "path": { "type": "string", "description": "Repository-relative folder to search in. Defaults to the project root." }
                },
                "required": ["pattern"]
            }),
        },
        ToolSpec {
            name: "write_file",
            description: "Create or overwrite a file in the project with the given content.",
            input_schema: object(&[
                ("path", "Repository-relative path to write."),
                ("content", "The complete new file contents."),
            ]),
        },
        ToolSpec {
            name: "edit_file",
            description: "Replace one exact, unique snippet in a file. Fails if the snippet is absent or appears more than once.",
            input_schema: object(&[
                ("path", "Repository-relative path to edit."),
                ("old", "The exact text to replace, including indentation."),
                ("new", "The replacement text."),
            ]),
        },
        ToolSpec {
            name: "run_command",
            description: "Run a shell command in the project root and return its combined output. Use for builds, tests and the project's own tooling.",
            input_schema: object(&[("command", "The command line to run.")]),
        },
    ]
}

fn object(fields: &[(&str, &str)]) -> serde_json::Value {
    let properties: serde_json::Map<String, serde_json::Value> = fields
        .iter()
        .map(|(name, description)| {
            (
                (*name).to_string(),
                serde_json::json!({ "type": "string", "description": description }),
            )
        })
        .collect();
    let required: Vec<&str> = fields.iter().map(|(name, _)| *name).collect();
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required
    })
}

/// Whether a tool mutates the machine and therefore needs approval before it runs.
///
/// Deliberately a function of the name, not a flag the model can influence: the model cannot talk its
/// way out of an approval.
pub fn needs_approval(name: &str) -> bool {
    matches!(name, "write_file" | "edit_file" | "run_command")
}

/// A one-line description of a call, for the permission card and the transcript's tool row.
pub fn title(call: &MsgBlock) -> String {
    let MsgBlock::ToolUse { name, input, .. } = call else {
        return String::new();
    };
    match name.as_str() {
        "run_command" => format!("Run {}", str_arg(input, "command").unwrap_or_default()),
        "read_file" | "list_dir" => {
            format!("{name} {}", str_arg(input, "path").unwrap_or_default())
        }
        "write_file" | "edit_file" => {
            format!("{name} {}", str_arg(input, "path").unwrap_or_default())
        }
        "grep" => format!("grep {}", str_arg(input, "pattern").unwrap_or_default()),
        other => other.to_string(),
    }
}

/// The detail line under the title: what the user is approving.
pub fn detail(call: &MsgBlock) -> String {
    let MsgBlock::ToolUse { name, input, .. } = call else {
        return String::new();
    };
    match name.as_str() {
        "run_command" => format!(
            "Working directory: the project root\n$ {}",
            str_arg(input, "command").unwrap_or_default()
        ),
        "write_file" => {
            let content = str_arg(input, "content").unwrap_or_default();
            format!(
                "{} ({} bytes)",
                str_arg(input, "path").unwrap_or_default(),
                content.len()
            )
        }
        "edit_file" => {
            let old = str_arg(input, "old").unwrap_or_default();
            let new = str_arg(input, "new").unwrap_or_default();
            format!(
                "{}\n- {} bytes\n+ {} bytes",
                str_arg(input, "path").unwrap_or_default(),
                old.len(),
                new.len()
            )
        }
        _ => title(call),
    }
}

fn str_arg(input: &serde_json::Value, name: &str) -> Option<String> {
    input
        .get(name)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// What a tool call produced.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutcome {
    /// The text handed back to the model, already truncated.
    pub content: String,
    /// A unified-ish patch, when the tool changed a file. The transcript renders it as a diff row.
    pub diff: Option<FileChange>,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub patch: String,
    pub added: u32,
    pub removed: u32,
}

impl ToolOutcome {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            diff: None,
            is_error: false,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            diff: None,
            is_error: true,
        }
    }

    pub fn with_diff(mut self, change: FileChange) -> Self {
        self.diff = Some(change);
        self
    }
}

/// How much of a file or command output is handed back to the model.
pub const MAX_OUTPUT_CHARS: usize = 200_000;
/// How much of one command's output is kept.
pub const MAX_COMMAND_CHARS: usize = 30_000;
/// How long one command may run before it is killed.
pub const COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
/// How many grep matches are reported.
pub const MAX_GREP_MATCHES: usize = 200;
/// Directories grep never descends into.
const SKIP_DIRS: [&str; 6] = [
    ".git",
    "node_modules",
    "target",
    "dist",
    ".grove",
    ".thinkrail",
];

/// Resolve a tool's `path` argument against the session directory, refusing anything that leaves it.
///
/// The check is on the *normalized* path components, so `../secrets` is refused whether or not it
/// exists, and an absolute path is only accepted when it is already inside the project.
pub fn resolve(cwd: &Path, requested: &str) -> Result<PathBuf, ToolOutcome> {
    let requested = requested.trim();
    let requested = if requested.is_empty() { "." } else { requested };
    let candidate = Path::new(requested);
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        cwd.join(candidate)
    };
    let normalized = normalize(&joined);
    let root = normalize(cwd);
    if !normalized.starts_with(&root) {
        return Err(ToolOutcome::error(format!(
            "path escapes the project: {requested}"
        )));
    }
    // The lexical check cannot see a symlink or a junction: a link inside the project can point
    // anywhere. Resolve what exists and re-check the real path; a path that does not exist yet is
    // checked through its nearest existing parent, which is where a link would have to live.
    let real_root = cwd.canonicalize().unwrap_or(root);
    if let Ok(real) = normalized.canonicalize() {
        if !real.starts_with(&real_root) {
            return Err(ToolOutcome::error(format!(
                "path escapes the project through a link: {requested}"
            )));
        }
    } else if let Some(parent) = normalized.parent() {
        if let Ok(real_parent) = parent.canonicalize() {
            if !real_parent.starts_with(&real_root) {
                return Err(ToolOutcome::error(format!(
                    "path escapes the project through a link: {requested}"
                )));
            }
        }
    }
    Ok(normalized)
}

/// Collapse `.` and `..` without touching the filesystem, so a non-existent target can be checked too.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Run one tool call. Async because `run_command` is.
pub async fn execute(cwd: &Path, call: &MsgBlock, allowed: bool) -> ToolOutcome {
    let MsgBlock::ToolUse { name, input, .. } = call else {
        return ToolOutcome::error("not a tool call");
    };
    if needs_approval(name) && !allowed {
        return ToolOutcome::error("the user declined this tool call");
    }
    let result = match name.as_str() {
        "read_file" => read_file(cwd, input),
        "list_dir" => list_dir(cwd, input),
        "grep" => grep(cwd, input),
        "write_file" => write_file(cwd, input),
        "edit_file" => edit_file(cwd, input),
        "run_command" => run_command(cwd, input).await,
        other => ToolOutcome::error(format!("unknown tool: {other}")),
    };
    result
}

fn arg(input: &serde_json::Value, name: &str) -> Result<String, ToolOutcome> {
    input
        .get(name)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ToolOutcome::error(format!("missing argument: {name}")))
}

fn read_file(cwd: &Path, input: &serde_json::Value) -> ToolOutcome {
    let path = match arg(input, "path") {
        Ok(path) => path,
        Err(outcome) => return outcome,
    };
    let resolved = match resolve(cwd, &path) {
        Ok(resolved) => resolved,
        Err(outcome) => return outcome,
    };
    match std::fs::read_to_string(&resolved) {
        Ok(text) => ToolOutcome::ok(truncate(&text, MAX_OUTPUT_CHARS)),
        Err(error) => ToolOutcome::error(format!("cannot read {path}: {error}")),
    }
}

fn list_dir(cwd: &Path, input: &serde_json::Value) -> ToolOutcome {
    let path = input
        .get("path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(".");
    let resolved = match resolve(cwd, path) {
        Ok(resolved) => resolved,
        Err(outcome) => return outcome,
    };
    let entries = match std::fs::read_dir(&resolved) {
        Ok(entries) => entries,
        Err(error) => return ToolOutcome::error(format!("cannot list {path}: {error}")),
    };
    let mut lines: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let suffix = match entry.file_type() {
            Ok(kind) if kind.is_dir() => "/",
            _ => "",
        };
        lines.push(format!("{name}{suffix}"));
    }
    lines.sort();
    if lines.is_empty() {
        return ToolOutcome::ok(format!("{path} is empty"));
    }
    ToolOutcome::ok(truncate(&lines.join("\n"), MAX_OUTPUT_CHARS))
}

fn grep(cwd: &Path, input: &serde_json::Value) -> ToolOutcome {
    let pattern = match arg(input, "pattern") {
        Ok(pattern) => pattern,
        Err(outcome) => return outcome,
    };
    if pattern.is_empty() {
        return ToolOutcome::error("pattern is empty");
    }
    let root = match resolve(
        cwd,
        input
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("."),
    ) {
        Ok(root) => root,
        Err(outcome) => return outcome,
    };
    let mut matches: Vec<String> = Vec::new();
    walk(&root, cwd, &pattern, &mut matches);
    if matches.is_empty() {
        return ToolOutcome::ok(format!("no matches for {pattern:?}"));
    }
    let capped = matches.len() > MAX_GREP_MATCHES;
    matches.truncate(MAX_GREP_MATCHES);
    let mut text = matches.join("\n");
    if capped {
        text.push_str(&format!("\n[capped at {MAX_GREP_MATCHES} matches]"));
    }
    ToolOutcome::ok(truncate(&text, MAX_OUTPUT_CHARS))
}

fn walk(root: &Path, cwd: &Path, pattern: &str, matches: &mut Vec<String>) {
    if matches.len() > MAX_GREP_MATCHES {
        return;
    }
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    let mut children: Vec<_> = entries.flatten().collect();
    children.sort_by_key(|entry| entry.file_name());
    for entry in children {
        if matches.len() > MAX_GREP_MATCHES {
            return;
        }
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => {
                let name = entry.file_name().to_string_lossy().to_string();
                if !SKIP_DIRS.contains(&name.as_str()) {
                    walk(&path, cwd, pattern, matches);
                }
            }
            Ok(kind) if kind.is_file() => {
                // A binary file is skipped, not reported: a failed UTF-8 read means "not text".
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let relative = path.strip_prefix(cwd).unwrap_or(&path);
                for (index, line) in text.lines().enumerate() {
                    if matches.len() > MAX_GREP_MATCHES {
                        return;
                    }
                    if line.contains(pattern) {
                        matches.push(format!(
                            "{}:{}: {}",
                            relative.display(),
                            index + 1,
                            line.trim()
                        ));
                    }
                }
            }
            _ => {}
        }
    }
}

fn write_file(cwd: &Path, input: &serde_json::Value) -> ToolOutcome {
    let path = match arg(input, "path") {
        Ok(path) => path,
        Err(outcome) => return outcome,
    };
    let content = match arg(input, "content") {
        Ok(content) => content,
        Err(outcome) => return outcome,
    };
    let resolved = match resolve(cwd, &path) {
        Ok(resolved) => resolved,
        Err(outcome) => return outcome,
    };
    let previous = std::fs::read_to_string(&resolved).unwrap_or_default();
    if let Some(parent) = resolved.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            return ToolOutcome::error(format!("cannot create {}: {error}", parent.display()));
        }
    }
    if let Err(error) = std::fs::write(&resolved, &content) {
        return ToolOutcome::error(format!("cannot write {path}: {error}"));
    }
    let change = diff_of(&path, &previous, &content);
    ToolOutcome::ok(format!("wrote {path} ({} bytes)", content.len())).with_diff(change)
}

fn edit_file(cwd: &Path, input: &serde_json::Value) -> ToolOutcome {
    let path = match arg(input, "path") {
        Ok(path) => path,
        Err(outcome) => return outcome,
    };
    let old = match arg(input, "old") {
        Ok(old) => old,
        Err(outcome) => return outcome,
    };
    let new = match arg(input, "new") {
        Ok(new) => new,
        Err(outcome) => return outcome,
    };
    if old.is_empty() {
        return ToolOutcome::error("old is empty; use write_file to create a file");
    }
    let resolved = match resolve(cwd, &path) {
        Ok(resolved) => resolved,
        Err(outcome) => return outcome,
    };
    let previous = match std::fs::read_to_string(&resolved) {
        Ok(text) => text,
        Err(error) => return ToolOutcome::error(format!("cannot read {path}: {error}")),
    };
    let occurrences = previous.matches(&old).count();
    match occurrences {
        0 => return ToolOutcome::error(format!("{path} does not contain that text")),
        1 => {}
        count => {
            return ToolOutcome::error(format!(
                "that text appears {count} times in {path}; include more context to make it unique"
            ))
        }
    }
    let updated = previous.replacen(&old, &new, 1);
    if let Err(error) = std::fs::write(&resolved, &updated) {
        return ToolOutcome::error(format!("cannot write {path}: {error}"));
    }
    let change = diff_of(&path, &previous, &updated);
    ToolOutcome::ok(format!("edited {path}")).with_diff(change)
}

async fn run_command(cwd: &Path, input: &serde_json::Value) -> ToolOutcome {
    let command = match arg(input, "command") {
        Ok(command) => command,
        Err(outcome) => return outcome,
    };
    if command.trim().is_empty() {
        return ToolOutcome::error("command is empty");
    }
    let mut builder = if cfg!(windows) {
        let mut builder = tokio::process::Command::new("cmd");
        builder.arg("/C").arg(&command);
        builder
    } else {
        let mut builder = tokio::process::Command::new("sh");
        builder.arg("-c").arg(&command);
        builder
    };
    builder
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        // A command that outlives the session must not keep the pipe open.
        .kill_on_drop(true);

    let child = builder.output();
    match tokio::time::timeout(COMMAND_TIMEOUT, child).await {
        Err(_) => ToolOutcome::error(format!(
            "command timed out after {}s",
            COMMAND_TIMEOUT.as_secs()
        )),
        Ok(Err(error)) => ToolOutcome::error(format!("cannot run command: {error}")),
        Ok(Ok(output)) => {
            let mut text = String::new();
            text.push_str(&String::from_utf8_lossy(&output.stdout));
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.trim().is_empty() {
                if !text.is_empty() {
                    text.push_str("\n--- stderr ---\n");
                }
                text.push_str(&stderr);
            }
            if text.trim().is_empty() {
                text = format!("(no output, exit {})", output.status.code().unwrap_or(-1));
            }
            let trimmed = truncate(&text, MAX_COMMAND_CHARS);
            if output.status.success() {
                ToolOutcome::ok(trimmed)
            } else {
                ToolOutcome {
                    content: format!("exit {}\n{trimmed}", output.status.code().unwrap_or(-1)),
                    diff: None,
                    is_error: true,
                }
            }
        }
    }
}

/// A per-line patch a user can read, plus the counts the transcript row shows.
pub fn diff_of(path: &str, before: &str, after: &str) -> FileChange {
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let mut patch = String::new();
    let mut added = 0u32;
    let mut removed = 0u32;
    // A minimal line-level diff is enough for a tool row: equal-length prefixes/suffixes are the
    // common case, and a full LCS would cost more than the row is worth.
    let mut prefix = 0;
    while prefix < before_lines.len()
        && prefix < after_lines.len()
        && before_lines[prefix] == after_lines[prefix]
    {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < before_lines.len().saturating_sub(prefix)
        && suffix < after_lines.len().saturating_sub(prefix)
        && before_lines[before_lines.len() - 1 - suffix]
            == after_lines[after_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }
    for line in &before_lines[prefix..before_lines.len() - suffix] {
        patch.push_str(&format!("-{line}\n"));
        removed += 1;
    }
    for line in &after_lines[prefix..after_lines.len() - suffix] {
        patch.push_str(&format!("+{line}\n"));
        added += 1;
    }
    if patch.is_empty() {
        patch.push_str("(no textual change)\n");
    }
    FileChange {
        path: path.to_string(),
        patch,
        added,
        removed,
    }
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let kept: String = text.chars().take(limit).collect();
    format!(
        "{kept}\n[truncated: {} of {} characters shown]",
        limit,
        text.chars().count()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, input: serde_json::Value) -> MsgBlock {
        MsgBlock::ToolUse {
            id: "c1".into(),
            name: name.into(),
            input,
        }
    }

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello\nworld\n").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/b.rs"), "fn main() {}\n").unwrap();
        dir
    }

    #[test]
    fn every_shipped_tool_is_classified_for_approval() {
        for spec in specs() {
            let _ = needs_approval(spec.name);
            assert!(
                !spec.description.is_empty(),
                "{} needs a description",
                spec.name
            );
        }
        let names: Vec<&str> = specs().iter().map(|spec| spec.name).collect();
        assert_eq!(names.len(), 6);
    }

    #[test]
    fn only_the_three_mutating_tools_need_approval() {
        for name in ["read_file", "list_dir", "grep"] {
            assert!(!needs_approval(name), "{name} is read-only");
        }
        for name in ["write_file", "edit_file", "run_command"] {
            assert!(needs_approval(name), "{name} changes the machine");
        }
    }

    #[test]
    fn a_path_inside_the_project_resolves_and_an_escape_does_not() {
        let dir = project();
        let inside = resolve(dir.path(), "src/b.rs").unwrap();
        assert!(inside.ends_with("b.rs"));

        for escape in ["../outside.txt", "../../etc/passwd", "src/../../no"] {
            let error = resolve(dir.path(), escape).unwrap_err();
            assert!(error.content.contains("escapes the project"), "{escape}");
        }
    }

    #[test]
    fn an_absolute_path_inside_the_project_is_allowed_and_outside_is_not() {
        let dir = project();
        let inside = dir.path().join("a.txt");
        assert!(resolve(dir.path(), &inside.to_string_lossy()).is_ok());
        assert!(resolve(dir.path(), "C:/Windows/win.ini").is_err());
    }

    #[tokio::test]
    async fn read_file_returns_content_and_refuses_an_escape() {
        let dir = project();
        let outcome = execute(
            dir.path(),
            &call("read_file", serde_json::json!({ "path": "a.txt" })),
            false,
        )
        .await;
        assert_eq!(outcome.content, "hello\nworld\n");
        assert!(!outcome.is_error);

        let escape = execute(
            dir.path(),
            &call("read_file", serde_json::json!({ "path": "../x" })),
            false,
        )
        .await;
        assert!(escape.is_error);
    }

    #[tokio::test]
    async fn a_mutating_tool_is_refused_without_approval() {
        let dir = project();
        let outcome = execute(
            dir.path(),
            &call(
                "write_file",
                serde_json::json!({ "path": "new.txt", "content": "x" }),
            ),
            false,
        )
        .await;
        assert!(outcome.is_error);
        assert!(outcome.content.contains("declined"));
        assert!(
            !dir.path().join("new.txt").exists(),
            "nothing is written without approval"
        );
    }

    #[tokio::test]
    async fn write_file_reports_a_diff_and_creates_parent_folders() {
        let dir = project();
        let outcome = execute(
            dir.path(),
            &call(
                "write_file",
                serde_json::json!({ "path": "deep/new.txt", "content": "one\ntwo\n" }),
            ),
            true,
        )
        .await;
        assert!(!outcome.is_error, "{}", outcome.content);
        let change = outcome.diff.expect("a write reports a diff");
        assert_eq!(change.added, 2);
        assert_eq!(change.removed, 0);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("deep/new.txt")).unwrap(),
            "one\ntwo\n"
        );
    }

    #[tokio::test]
    async fn edit_file_is_exact_unique_and_reports_what_moved() {
        let dir = project();
        let outcome = execute(
            dir.path(),
            &call(
                "edit_file",
                serde_json::json!({ "path": "a.txt", "old": "world", "new": "there" }),
            ),
            true,
        )
        .await;
        assert!(!outcome.is_error, "{}", outcome.content);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "hello\nthere\n"
        );
        let change = outcome.diff.unwrap();
        assert_eq!((change.added, change.removed), (1, 1));
    }

    #[tokio::test]
    async fn an_ambiguous_or_absent_snippet_is_refused_rather_than_guessed() {
        let dir = project();
        std::fs::write(dir.path().join("dup.txt"), "same\nsame\n").unwrap();
        let ambiguous = execute(
            dir.path(),
            &call(
                "edit_file",
                serde_json::json!({ "path": "dup.txt", "old": "same", "new": "x" }),
            ),
            true,
        )
        .await;
        assert!(ambiguous.is_error);
        assert!(ambiguous.content.contains("appears 2 times"));

        let absent = execute(
            dir.path(),
            &call(
                "edit_file",
                serde_json::json!({ "path": "a.txt", "old": "nope", "new": "x" }),
            ),
            true,
        )
        .await;
        assert!(absent.is_error);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("dup.txt")).unwrap(),
            "same\nsame\n",
            "a refused edit changes nothing"
        );
    }

    #[tokio::test]
    async fn grep_reports_file_and_line_and_skips_noise_directories() {
        let dir = project();
        std::fs::create_dir_all(dir.path().join("node_modules/pkg")).unwrap();
        std::fs::write(dir.path().join("node_modules/pkg/x.js"), "needle\n").unwrap();
        let outcome = execute(
            dir.path(),
            &call("grep", serde_json::json!({ "pattern": "needle" })),
            false,
        )
        .await;
        assert!(
            outcome.content.starts_with("no matches"),
            "node_modules is skipped: {}",
            outcome.content
        );

        let outcome = execute(
            dir.path(),
            &call("grep", serde_json::json!({ "pattern": "main" })),
            false,
        )
        .await;
        assert!(outcome.content.contains("src"), "{}", outcome.content);
        assert!(outcome.content.contains(":1:"), "{}", outcome.content);
    }

    #[tokio::test]
    async fn list_dir_marks_folders_and_hides_nothing_else() {
        let dir = project();
        let outcome = execute(
            dir.path(),
            &call("list_dir", serde_json::json!({ "path": "." })),
            false,
        )
        .await;
        assert!(outcome.content.contains("src/"), "{}", outcome.content);
        assert!(outcome.content.contains("a.txt"));
    }

    #[tokio::test]
    async fn run_command_returns_output_and_its_exit_status() {
        let dir = project();
        let ok = execute(
            dir.path(),
            &call("run_command", serde_json::json!({ "command": "echo hi" })),
            true,
        )
        .await;
        assert!(!ok.is_error, "{}", ok.content);
        assert!(ok.content.contains("hi"));

        let failed = execute(
            dir.path(),
            &call("run_command", serde_json::json!({ "command": "exit 3" })),
            true,
        )
        .await;
        assert!(failed.is_error);
        assert!(failed.content.starts_with("exit 3"), "{}", failed.content);
    }

    #[test]
    fn a_permission_card_says_what_would_run() {
        let title = title(&call(
            "run_command",
            serde_json::json!({ "command": "npm test" }),
        ));
        assert_eq!(title, "Run npm test");
        let detail = detail(&call(
            "run_command",
            serde_json::json!({ "command": "npm test" }),
        ));
        assert!(detail.contains("npm test"));
    }

    #[test]
    fn a_diff_counts_only_changed_lines() {
        let change = diff_of("a.txt", "keep\nold\ntail\n", "keep\nnew\ntail\n");
        assert_eq!((change.added, change.removed), (1, 1));
        assert!(change.patch.contains("-old"));
        assert!(change.patch.contains("+new"));
        assert!(!change.patch.contains("keep"));
    }
}
