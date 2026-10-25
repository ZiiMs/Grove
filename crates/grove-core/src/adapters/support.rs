//! The helpers every process-backed adapter needs, in one place.
//!
//! These were copy-pasted into five adapters, which is exactly the kind of duplication that drifts: a
//! fix to the Windows `taskkill` path or to the prompt layout has to land five times, and a test that
//! pins one copy proves nothing about the others. Nothing here knows a harness's protocol — an
//! adapter is still the only place its wire format lives.
//!
//! Each adapter calls these instead of carrying its own copy:
//! [`command_from_config`], [`render_prompt`], [`truncate`], [`find_on_path`],
//! [`kill_tree_by_pid`] and [`kill_process_tree`].

use crate::adapters::ContextDoc;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};

/// How long a killed child gets to be reaped before the adapter stops waiting for it.
///
/// A harness that ignores the kill must never hold a session hostage; the operating system reaps it
/// once the adapter task ends.
pub const REAP_TIMEOUT: Duration = Duration::from_secs(5);

/// Resolve `config.command` (then `config.args`) against an adapter's default command.
///
/// The override is the documented hook a user — or a test pointing at `fixtures/harnesses/*.mjs` —
/// uses to launch any binary. Precedence is explicit: a non-empty `config.command` wins over the
/// adapter's default, and `config.args` is the **extra** argv appended to the launch form, never a
/// replacement for it. A missing, blank or non-string command keeps the default; non-string array
/// entries are skipped rather than failing the launch.
pub fn command_from_config(config: &Value, default_command: &str) -> (String, Vec<String>) {
    let command = config
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .unwrap_or(default_command)
        .to_string();
    let args = config
        .get("args")
        .and_then(Value::as_array)
        .map(|args| {
            args.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    (command, args)
}

/// Render the first prompt: the context documents (pinned specs, the plan) in markdown, then the
/// prompt itself under a `---` rule.
///
/// With no documents this is the prompt unchanged (and `None` stays `None`, so an adapter never sends
/// an empty turn). With documents and no prompt the documents are the whole prompt — that is how a
/// resumed session re-establishes knowledge for a harness that cannot resume.
pub fn render_prompt(prompt: Option<&str>, documents: &[ContextDoc]) -> Option<String> {
    if documents.is_empty() {
        return prompt.map(str::to_string);
    }
    let mut rendered = String::from("# Context\n\n");
    for document in documents {
        rendered.push_str(&format!(
            "## {}\n\n<!-- id: {} -->\n\n",
            document.title, document.id
        ));
        rendered.push_str(document.body.trim_end());
        rendered.push_str("\n\n");
    }
    if let Some(prompt) = prompt.map(str::trim).filter(|prompt| !prompt.is_empty()) {
        rendered.push_str("---\n\n");
        rendered.push_str(prompt);
        rendered.push('\n');
    }
    Some(rendered)
}

/// Cut `text` to `limit` characters, marking the cut with an ellipsis so a transcript never pretends
/// a summary is the whole output. Counts characters, not bytes, so multi-byte text is not split.
pub fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(limit).collect();
    truncated.push('…');
    truncated
}

/// Resolve a binary the way a shell would: an explicit path as given, otherwise `PATH` + `PATHEXT`.
pub fn find_on_path(binary: &str) -> Option<PathBuf> {
    if binary.contains('/') || binary.contains('\\') || Path::new(binary).is_absolute() {
        let path = PathBuf::from(binary);
        return path.is_file().then_some(path);
    }
    let search_path = std::env::var_os("PATH")?;
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .ok()
            .map(|raw| {
                raw.split(';')
                    .map(str::trim)
                    .filter(|extension| !extension.is_empty())
                    .map(|extension| extension.to_ascii_lowercase())
                    .collect()
            })
            .filter(|extensions: &Vec<String>| !extensions.is_empty())
            .unwrap_or_else(|| vec![".exe".into(), ".cmd".into(), ".bat".into(), ".com".into()])
    } else {
        vec![String::new()]
    };
    for directory in std::env::split_paths(&search_path) {
        for extension in &extensions {
            let candidate = directory.join(format!("{binary}{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Kill a child and its whole process tree, then wait (bounded) for it to be reaped.
///
/// A harness spawns tools of its own, and `Child::kill` reaches the direct child only, so every
/// adapter ends a session the same way: [`kill_tree_by_pid`] first, then the direct kill as the
/// backstop. The wait is bounded by [`REAP_TIMEOUT`].
pub async fn kill_process_tree(pid: Option<u32>, child: &mut Child) {
    if let Some(pid) = pid {
        kill_tree_by_pid(pid).await;
    }
    let _ = child.kill().await;
    let _ = tokio::time::timeout(REAP_TIMEOUT, child.wait()).await;
}

/// Kill a process tree by pid. Windows uses `taskkill /T /F`; elsewhere the child is spawned as a
/// process-group leader, so the negative pid is the whole tree. A failure is fine: the direct kill in
/// [`kill_process_tree`] still runs.
#[cfg(windows)]
pub async fn kill_tree_by_pid(pid: u32) {
    let pid = pid.to_string();
    let _ = Command::new("taskkill")
        .args(["/PID", pid.as_str(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
}

#[cfg(not(windows))]
pub async fn kill_tree_by_pid(pid: u32) {
    let group = format!("-{pid}");
    let _ = Command::new("kill")
        .args(["-KILL", group.as_str()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::ContextDoc;

    fn documents() -> Vec<ContextDoc> {
        vec![ContextDoc {
            id: "grove-architecture".into(),
            title: "Grove — architecture".into(),
            body: "# Shape\n\nA Tauri app.\n".into(),
        }]
    }

    #[test]
    fn command_from_config_prefers_the_override_and_keeps_argv_extras() {
        let config = serde_json::json!({
            "command": "  node  ",
            "args": ["fixtures/harnesses/claude_fake.mjs", 7, "--mode", "happy"],
        });
        let (command, args) = command_from_config(&config, "claude");
        assert_eq!(command, "node");
        assert_eq!(
            args,
            vec!["fixtures/harnesses/claude_fake.mjs", "--mode", "happy"],
            "args are appended verbatim; non-strings are skipped"
        );
    }

    #[test]
    fn command_from_config_falls_back_to_the_default_command() {
        for config in [
            serde_json::json!({}),
            serde_json::json!({ "command": "   " }),
            serde_json::json!({ "command": 42 }),
            serde_json::json!({ "command": null, "args": ["x"] }),
        ] {
            let (command, args) = command_from_config(&config, "codex");
            assert_eq!(command, "codex", "config: {config}");
            assert_eq!(
                args,
                if config.get("args").is_some() {
                    vec!["x"]
                } else {
                    vec![]
                }
            );
        }
    }

    #[test]
    fn render_prompt_without_documents_is_the_prompt_unchanged() {
        assert_eq!(render_prompt(Some("plain"), &[]).as_deref(), Some("plain"));
        assert_eq!(render_prompt(None, &[]), None);
    }

    #[test]
    fn render_prompt_prepends_documents_and_separates_the_prompt() {
        let rendered = render_prompt(Some("ship it"), &documents()).expect("a prompt");
        assert!(rendered.starts_with("# Context\n\n## Grove — architecture\n\n"));
        assert!(rendered.contains("<!-- id: grove-architecture -->"));
        assert!(rendered.contains("A Tauri app.\n\n---\n\nship it\n"));
    }

    #[test]
    fn render_prompt_with_only_documents_still_produces_a_prompt() {
        let rendered = render_prompt(None, &documents()).expect("documents are the prompt");
        assert!(rendered.starts_with("# Context"));
        assert!(!rendered.contains("---\n\nship it"));
        assert_eq!(
            render_prompt(Some("   "), &documents()).expect("documents are the prompt"),
            rendered,
            "a blank prompt adds nothing"
        );
    }

    #[test]
    fn truncate_marks_what_it_cut() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("exactly", 7), "exactly");
        assert_eq!(truncate("a long line", 4), "a lo…");
        assert_eq!(
            truncate("héllo wörld", 5),
            "héllo…",
            "counts characters, not bytes"
        );
    }

    #[test]
    fn find_on_path_resolves_a_relative_path_and_refuses_a_missing_one() {
        let directory = tempfile::tempdir().expect("tempdir");
        let program = directory
            .path()
            .join(if cfg!(windows) { "probe.exe" } else { "probe" });
        std::fs::write(&program, b"not really a program").expect("write the probe");
        let shown = program.display().to_string();
        assert_eq!(find_on_path(&shown), Some(program.clone()));
        assert_eq!(
            find_on_path(&format!("{shown}.missing")),
            None,
            "an explicit path that does not exist is never guessed"
        );
        assert_eq!(
            find_on_path("grove-definitely-not-on-path-9f3c"),
            None,
            "an unresolvable name is None, not a panic"
        );
    }

    #[tokio::test]
    async fn kill_process_tree_kills_a_spawned_child() {
        let (program, args): (&str, Vec<&str>) = if cfg!(windows) {
            ("cmd", vec!["/C", "ping -n 30 127.0.0.1 > NUL"])
        } else {
            ("sleep", vec!["30"])
        };
        let mut child = Command::new(program)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn a child to kill");
        let pid = child.id();
        kill_process_tree(pid, &mut child).await;
        let status = child.try_wait().expect("try_wait");
        assert!(
            status.is_some(),
            "the child must be reaped, not left running (try_wait: {status:?})"
        );
    }

    #[tokio::test]
    async fn kill_tree_by_pid_tolerates_an_unknown_pid() {
        // No assertion beyond "does not panic or hang": a stale pid is a normal race on cancel.
        kill_tree_by_pid(u32::MAX).await;
        let (program, args): (&str, Vec<&str>) = if cfg!(windows) {
            ("cmd", vec!["/C", "exit 0"])
        } else {
            ("true", vec![])
        };
        let mut child = Command::new(program).args(args).spawn().expect("spawn");
        kill_process_tree(None, &mut child).await;
    }
}
