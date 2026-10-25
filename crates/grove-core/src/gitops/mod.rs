//! gitops — the only place Grove touches `git` or `gh`.
//!
//! Every subprocess crosses [`CommandRunner`]: a program name plus an argument *list*, never a shell
//! string, so quoting is never a correctness problem and a Windows path cannot be re-parsed. Tests
//! substitute a fake runner (see `tests/gitops.rs`) to drive a scripted `gh` and assert the exact argv.
//!
//! Missing binaries become [`IpcError::unavailable`] with the reason — never a panic, never a silent
//! no-op. Read-only commands run with `GIT_OPTIONAL_LOCKS=0` so a status read does not touch the
//! index, and paths handed to git/gh are validated to stay inside the repository.
//!
//! Output is bounded: [`truncate_output`] cuts on a character boundary, appends [`TRUNCATION_MARKER`],
//! and the DTO carries `truncated: true` so the UI can say so instead of pretending.

mod diff;
mod pr;
mod repo;
mod types;

pub use types::{
    DiffRequest, DiffTarget, GitBranch, GitDetection, GitDiff, GitStatus, GitStatusFile,
    GitWorktree, PrRequest, PrResult,
};

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use crate::ipc::{IpcError, IpcResult};

/// Upper bound on any text gitops returns to the webview (a diff, a status document).
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;

/// Appended to output that crossed [`MAX_OUTPUT_BYTES`]; `truncated: true` is the machine-readable half.
pub const TRUNCATION_MARKER: &str = "\n\n--- output truncated: ";

/// One process to launch. `args` is a list: no shell, so no quoting rules to get wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
}

/// What a process printed. Both streams are lossy UTF-8: the wire is JSON, not bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.code == 0
    }

    /// The tail of stderr, for an error `detail`. Bounded so a chatty tool cannot balloon the wire.
    fn stderr_tail(&self) -> String {
        tail(&self.stderr, 500)
    }
}

/// The single seam every subprocess crosses. Tests implement this to fake `gh` (and to record argv).
pub trait CommandRunner: Send + Sync {
    fn run(&self, spec: &CommandSpec) -> IpcResult<CommandOutput>;
}

/// Real processes. A missing binary is reported as `unavailable`, never panicked on.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, spec: &CommandSpec) -> IpcResult<CommandOutput> {
        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        let output = match command.output() {
            Ok(output) => output,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(IpcError::unavailable(format!(
                    "{} is not installed or not on PATH",
                    spec.program
                ))
                .with_detail(err.to_string()));
            }
            Err(err) => {
                return Err(IpcError::io(format!(
                    "failed to run {}: {err}",
                    spec.program
                )));
            }
        };
        Ok(CommandOutput {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// The gitops service. Cheap to clone: the runner is shared and the program names are strings.
#[derive(Clone)]
pub struct Git {
    runner: Arc<dyn CommandRunner>,
    git_bin: String,
    gh_bin: String,
}

impl fmt::Debug for Git {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Git")
            .field("git", &self.git_bin)
            .field("gh", &self.gh_bin)
            .finish_non_exhaustive()
    }
}

impl Git {
    /// The real thing: `git` and `gh` resolved from `PATH`.
    pub fn system() -> Self {
        Self::with_runner(Arc::new(SystemCommandRunner))
    }

    pub fn with_runner(runner: Arc<dyn CommandRunner>) -> Self {
        Self::with_binaries(runner, "git", "gh")
    }

    /// Program names are injectable so a test can prove what an absent `gh` or `git` looks like.
    pub fn with_binaries(
        runner: Arc<dyn CommandRunner>,
        git_bin: impl Into<String>,
        gh_bin: impl Into<String>,
    ) -> Self {
        Self {
            runner,
            git_bin: git_bin.into(),
            gh_bin: gh_bin.into(),
        }
    }

    fn run_args(&self, cwd: &Path, args: &[String]) -> IpcResult<CommandOutput> {
        let spec = CommandSpec {
            program: self.git_bin.clone(),
            args: args.to_vec(),
            cwd: Some(cwd.to_path_buf()),
            env: git_env(),
        };
        self.runner.run(&spec)
    }

    fn run(&self, cwd: &Path, args: &[&str]) -> IpcResult<CommandOutput> {
        let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        self.run_args(cwd, &owned)
    }

    /// Resolve the repository root for a requested path (or the process cwd), so every later command
    /// runs with the root as its working directory and cannot wander outside it.
    fn discover(&self, repo: Option<&Path>) -> IpcResult<PathBuf> {
        let start = match repo {
            Some(path) => path.to_path_buf(),
            None => std::env::current_dir()
                .map_err(|err| IpcError::io(format!("cannot read the current directory: {err}")))?,
        };
        let output = self.run(&start, &["rev-parse", "--show-toplevel"])?;
        if !output.success() {
            let detail = output.stderr_tail();
            let message = if detail.to_lowercase().contains("not a git repository") {
                format!("not inside a git repository: {}", start.display())
            } else {
                "could not resolve the git repository root".to_string()
            };
            return Err(IpcError::invalid(message).with_detail(detail));
        }
        let root = output.stdout.lines().next().unwrap_or_default().trim();
        if root.is_empty() {
            return Err(IpcError::invalid(format!(
                "not inside a git repository: {}",
                start.display()
            )));
        }
        Ok(PathBuf::from(root))
    }
}

fn git_env() -> Vec<(String, String)> {
    vec![
        // Never block on a credential prompt; report the failure instead.
        ("GIT_TERMINAL_PROMPT".to_string(), "0".to_string()),
        // A status/diff read must not refresh the index (a write) as a side effect.
        ("GIT_OPTIONAL_LOCKS".to_string(), "0".to_string()),
    ]
}

/// Cut `text` to `limit` bytes on a character boundary, appending [`TRUNCATION_MARKER`].
/// Returns the text unchanged when it already fits.
pub(crate) fn truncate_output(text: &str, limit: usize) -> (String, bool) {
    if text.len() <= limit {
        return (text.to_string(), false);
    }
    let mut cut = limit;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let omitted = text.len() - cut;
    let mut bounded = String::with_capacity(cut + 64);
    bounded.push_str(&text[..cut]);
    bounded.push_str(TRUNCATION_MARKER);
    bounded.push_str(&format!("{omitted} of {} bytes omitted ---\n", text.len()));
    (bounded, true)
}

/// `Some(trimmed)` when the string carries a value, otherwise `None`. Keeps empty UI fields out of
/// the DTOs as `null` rather than `""`.
pub(crate) fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Bound a value for an error `detail`.
pub(crate) fn tail(text: &str, limit: usize) -> String {
    let text = text.trim();
    if text.len() <= limit {
        return text.to_string();
    }
    let mut start = text.len() - limit;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &text[start..])
}

/// A failed git process. `unavailable` is reserved for a missing binary, so a refused command is
/// `io` with git's own stderr as the detail.
pub(crate) fn git_failure(action: &str, output: &CommandOutput) -> IpcError {
    IpcError::io(format!("git {action} failed (exit code {})", output.code))
        .with_detail(output.stderr_tail())
}

/// Reject a ref that would be read as an option, e.g. `--output=…`; git never allows such a name.
pub(crate) fn validate_ref(kind: &str, value: &str) -> IpcResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(IpcError::invalid(format!("{kind} must not be empty")));
    }
    if value.starts_with('-') {
        return Err(IpcError::invalid(format!(
            "{kind} must not start with '-': {value}"
        )));
    }
    Ok(value.to_string())
}

/// Repository-relative paths only. This is what keeps `git add`/`git commit` inside the repository.
pub(crate) fn validate_path(path: &str) -> IpcResult<()> {
    if path.trim().is_empty() {
        return Err(IpcError::invalid("path must not be empty"));
    }
    let candidate = Path::new(path);
    // `Path::is_absolute` is platform-specific; reject Windows drive/UNC forms on every platform too.
    let looks_absolute = candidate.is_absolute()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.as_bytes().get(1) == Some(&b':');
    if looks_absolute {
        return Err(IpcError::invalid(format!(
            "path must be relative to the repository: {path}"
        )));
    }
    if candidate
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(IpcError::invalid(format!(
            "path must not leave the repository: {path}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_output_is_returned_untouched() {
        assert_eq!(
            truncate_output("diff --git a b", 64),
            ("diff --git a b".to_string(), false)
        );
    }

    #[test]
    fn long_output_is_cut_on_a_character_boundary_with_a_marker() {
        let text = "héllo wörld ".repeat(100);
        let (bounded, truncated) = truncate_output(&text, 65);
        assert!(truncated);
        assert!(bounded.len() > 65, "marker is appended after the cut");
        assert!(bounded.contains(TRUNCATION_MARKER));
        let kept = bounded.find(TRUNCATION_MARKER).expect("marker");
        assert_eq!(
            kept, 64,
            "cut backs off to a character boundary under the limit"
        );
        assert!(
            bounded.contains(&format!(
                "{} of {} bytes omitted",
                text.len() - kept,
                text.len()
            )),
            "{bounded}"
        );
        // The retained prefix is unchanged (and still valid UTF-8, since it is a String).
        assert!(text.starts_with(&bounded[..kept]));
    }

    #[test]
    fn non_empty_trims_and_rejects_blank() {
        assert_eq!(non_empty(" main "), Some("main".to_string()));
        assert_eq!(non_empty("   "), None);
        assert_eq!(non_empty(""), None);
    }

    #[test]
    fn path_validation_rejects_escapes_and_absolutes() {
        assert!(validate_path("src/main.ts").is_ok());
        assert!(validate_path("src/../main.ts").is_err());
        assert!(validate_path("../outside.txt").is_err());
        assert!(validate_path("/etc/passwd").is_err());
        assert!(validate_path("C:/Windows/system.ini").is_err());
        assert!(validate_path("  ").is_err());
    }

    #[test]
    fn ref_validation_rejects_option_lookalikes() {
        assert_eq!(validate_ref("branch", " main ").unwrap(), "main");
        assert!(validate_ref("branch", "").is_err());
        assert!(validate_ref("base", "--upload-pack=evil").is_err());
    }
}
