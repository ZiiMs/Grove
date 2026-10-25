//! gitops against real git repositories in temp dirs, and `gh` behind a fake runner.
//!
//! Every test here drives the real `git` CLI (there is no libgit2 in Grove), while `gh` is scripted so
//! no test needs a GitHub account or the network. The fake records the exact argv it was handed, which
//! is how "no shell strings; args as a list" is proven rather than asserted in prose.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use grove_core::gitops::{
    CommandOutput, CommandRunner, CommandSpec, DiffRequest, DiffTarget, Git, GitStatus, PrRequest,
    SystemCommandRunner, MAX_OUTPUT_BYTES,
};
use grove_core::ipc::{ErrorCode, IpcResult};
use tempfile::TempDir;

// ---------------------------------------------------------------------------------------------
// Test fixtures: a real repository, a real bare remote, and a fake `gh`.
// ---------------------------------------------------------------------------------------------

fn run_git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|err| panic!("failed to spawn git {args:?}: {err}"));
    assert!(
        output.status.success(),
        "git {args:?} failed in {}: {}",
        cwd.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A throwaway repository with a local identity, so commits never depend on the machine's config.
struct Repo {
    dir: TempDir,
}

impl Repo {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let repo = Self { dir };
        repo.git(&["init", "--quiet", "--initial-branch=main"]);
        repo.git(&["config", "user.email", "tester@grove.test"]);
        repo.git(&["config", "user.name", "Grove Tester"]);
        // Keep committed bytes identical to what the test writes, on every platform.
        repo.git(&["config", "core.autocrlf", "false"]);
        repo
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.path().join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dir");
        }
        std::fs::write(path, contents).expect("write file");
    }

    fn git(&self, args: &[&str]) -> String {
        run_git(self.path(), args)
    }

    fn commit_all(&self, message: &str) {
        self.git(&["add", "--all"]);
        self.git(&["commit", "--quiet", "--message", message]);
    }
}

fn bare_repo() -> TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    run_git(
        dir.path(),
        &["init", "--quiet", "--bare", "--initial-branch=main"],
    );
    dir
}

fn find_file<'a>(status: &'a GitStatus, path: &str) -> &'a grove_core::gitops::GitStatusFile {
    status
        .files
        .iter()
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("status has no entry for {path}: {:?}", status.files))
}

/// Delegates every non-`gh` process to the real runner, scripts the `gh` outcome, and records argv.
struct GhStub {
    response: Mutex<Option<IpcResult<CommandOutput>>>,
    specs: Mutex<Vec<CommandSpec>>,
}

impl GhStub {
    fn new(response: IpcResult<CommandOutput>) -> Self {
        Self {
            response: Mutex::new(Some(response)),
            specs: Mutex::new(Vec::new()),
        }
    }

    fn gh_specs(&self) -> Vec<CommandSpec> {
        self.specs
            .lock()
            .expect("specs lock")
            .iter()
            .filter(|spec| spec.program == "gh")
            .cloned()
            .collect()
    }
}

impl CommandRunner for GhStub {
    fn run(&self, spec: &CommandSpec) -> IpcResult<CommandOutput> {
        self.specs.lock().expect("specs lock").push(spec.clone());
        if spec.program == "gh" {
            return self
                .response
                .lock()
                .expect("response lock")
                .take()
                .expect("gh was invoked more than once");
        }
        SystemCommandRunner.run(spec)
    }
}

/// Records every spec and delegates to the real runner.
struct RecordingRunner {
    specs: Mutex<Vec<CommandSpec>>,
}

impl RecordingRunner {
    fn new() -> Self {
        Self {
            specs: Mutex::new(Vec::new()),
        }
    }

    fn specs(&self) -> Vec<CommandSpec> {
        self.specs.lock().expect("specs lock").clone()
    }
}

impl CommandRunner for RecordingRunner {
    fn run(&self, spec: &CommandSpec) -> IpcResult<CommandOutput> {
        self.specs.lock().expect("specs lock").push(spec.clone());
        SystemCommandRunner.run(spec)
    }
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path)
        .unwrap_or_else(|err| panic!("canonicalize {}: {err}", path.display()))
}

// ---------------------------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------------------------

#[test]
fn status_reports_clean_dirty_staged_untracked_and_the_current_branch() {
    let repo = Repo::new();
    repo.write("tracked.txt", "one\n");
    repo.commit_all("init");
    let git = Git::system();

    let status = git.status(Some(repo.path())).expect("clean status");
    assert!(status.clean, "{status:?}");
    assert!(status.files.is_empty());
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert_eq!(status.head.as_deref().map(str::len), Some(40));
    assert!(!status.truncated);
    assert_eq!(
        canonical(Path::new(&status.repo_root)),
        canonical(repo.path())
    );

    repo.write("tracked.txt", "one\ntwo\n");
    let status = git.status(Some(repo.path())).expect("dirty status");
    let tracked = find_file(&status, "tracked.txt");
    assert_eq!(
        (tracked.index.as_str(), tracked.worktree.as_str()),
        (" ", "M")
    );
    assert!(!tracked.staged && !tracked.untracked);
    assert!(!status.clean);

    repo.write("staged.txt", "new\n");
    repo.git(&["add", "staged.txt"]);
    let status = git.status(Some(repo.path())).expect("staged status");
    let staged = find_file(&status, "staged.txt");
    assert_eq!(
        (staged.index.as_str(), staged.worktree.as_str()),
        ("A", " ")
    );
    assert!(staged.staged && !staged.untracked);

    repo.write("untracked.txt", "loose\n");
    let status = git.status(Some(repo.path())).expect("untracked status");
    let untracked = find_file(&status, "untracked.txt");
    assert!(untracked.untracked);
    assert!(!untracked.staged);

    repo.git(&["checkout", "--quiet", "-b", "feature/x"]);
    let status = git.status(Some(repo.path())).expect("branch status");
    assert_eq!(status.branch.as_deref(), Some("feature/x"));
}

#[test]
fn status_reports_a_rename_with_its_previous_path() {
    let repo = Repo::new();
    repo.write("old.txt", "stable\n");
    repo.commit_all("init");
    repo.git(&["mv", "old.txt", "new.txt"]);

    let status = Git::system().status(Some(repo.path())).expect("status");
    let renamed = find_file(&status, "new.txt");
    assert_eq!(renamed.from.as_deref(), Some("old.txt"));
    assert_eq!(renamed.index, "R");
}

#[test]
fn status_reports_an_empty_repository_without_a_head() {
    let repo = Repo::new();
    repo.write("fresh.txt", "hello\n");

    let status = Git::system().status(Some(repo.path())).expect("status");
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert_eq!(status.head, None);
    let fresh = find_file(&status, "fresh.txt");
    assert!(fresh.untracked);
}

#[test]
fn status_on_a_non_repository_is_invalid_not_a_panic() {
    let dir = tempfile::tempdir().expect("temp dir");
    let err = Git::system()
        .status(Some(dir.path()))
        .expect_err("not a git repository");
    assert_eq!(err.code, ErrorCode::Invalid);
    assert!(err.message.contains("git repository"), "{err:?}");
}

// ---------------------------------------------------------------------------------------------
// Branches and worktrees
// ---------------------------------------------------------------------------------------------

#[test]
fn branches_report_local_remote_current_and_ahead_behind() {
    let repo = Repo::new();
    let bare = bare_repo();
    repo.write("a.txt", "a\n");
    repo.commit_all("init");
    repo.git(&[
        "remote",
        "add",
        "origin",
        bare.path().to_str().expect("utf-8 path"),
    ]);
    repo.git(&["push", "--quiet", "--set-upstream", "origin", "main"]);
    repo.git(&["branch", "local-only"]);
    repo.git(&["push", "--quiet", "origin", "main:remote-only"]);

    repo.write("b.txt", "b\n");
    repo.commit_all("ahead");

    // A second clone pushes a commit the local repo has not seen yet: one ahead, one behind.
    let clone_root = tempfile::tempdir().expect("temp dir");
    let clone = clone_root.path().join("clone");
    let output = Command::new("git")
        .args(["clone", "--quiet"])
        .arg(bare.path())
        .arg(&clone)
        .output()
        .expect("spawn git clone");
    assert!(
        output.status.success(),
        "git clone failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    run_git(&clone, &["config", "user.email", "tester@grove.test"]);
    run_git(&clone, &["config", "user.name", "Grove Tester"]);
    std::fs::write(clone.join("c.txt"), "c\n").expect("write clone file");
    run_git(&clone, &["add", "--all"]);
    run_git(&clone, &["commit", "--quiet", "--message", "remote commit"]);
    run_git(&clone, &["push", "--quiet", "origin", "main"]);
    repo.git(&["fetch", "--quiet", "origin"]);

    let branches = Git::system().branches(Some(repo.path())).expect("branches");

    let main = branches
        .iter()
        .find(|branch| branch.name == "main")
        .expect("local main");
    assert!(main.current);
    assert_eq!(main.remote, None);
    assert_eq!(main.upstream.as_deref(), Some("origin/main"));
    assert_eq!((main.ahead, main.behind), (1, 1));

    let origin_main = branches
        .iter()
        .find(|branch| branch.name == "origin/main")
        .expect("remote main");
    assert_eq!(origin_main.remote.as_deref(), Some("origin"));
    assert!(!origin_main.current);
    assert_eq!((origin_main.ahead, origin_main.behind), (0, 0));

    let local_only = branches
        .iter()
        .find(|branch| branch.name == "local-only")
        .expect("local-only branch");
    assert_eq!(local_only.upstream, None);
    assert_eq!((local_only.ahead, local_only.behind), (0, 0));

    let remote_only = branches
        .iter()
        .find(|branch| branch.name == "origin/remote-only")
        .expect("remote-only branch");
    assert_eq!(remote_only.remote.as_deref(), Some("origin"));
}

#[test]
fn worktrees_list_the_main_and_linked_worktree_and_status_discovers_from_a_worktree() {
    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("init");

    let parent = tempfile::tempdir().expect("temp dir");
    let linked = parent.path().join("linked");
    repo.git(&[
        "worktree",
        "add",
        "--quiet",
        linked.to_str().expect("utf-8 path"),
        "-b",
        "wt-branch",
    ]);

    let worktrees = Git::system()
        .worktrees(Some(repo.path()))
        .expect("worktrees");
    assert_eq!(worktrees.len(), 2);
    let main = worktrees
        .iter()
        .find(|worktree| worktree.current)
        .expect("one current worktree");
    assert_eq!(main.branch.as_deref(), Some("main"));
    assert!(!main.bare && !main.detached);

    let linked_entry = worktrees
        .iter()
        .find(|worktree| worktree.branch.as_deref() == Some("wt-branch"))
        .expect("linked worktree");
    assert!(!linked_entry.current);
    assert_eq!(canonical(Path::new(&linked_entry.path)), canonical(&linked));

    // Discovery from inside the linked worktree resolves to that worktree's root, and git reports the
    // same branch the worktree has checked out.
    let status = Git::system()
        .status(Some(&linked))
        .expect("status from worktree");
    assert_eq!(status.branch.as_deref(), Some("wt-branch"));
    assert_eq!(canonical(Path::new(&status.repo_root)), canonical(&linked));
}

// ---------------------------------------------------------------------------------------------
// Diff
// ---------------------------------------------------------------------------------------------

#[test]
fn diff_covers_the_working_tree_and_a_branch_against_its_base() {
    let repo = Repo::new();
    repo.write("file.txt", "line one\nline two\n");
    repo.commit_all("base");
    repo.write("file.txt", "line one changed\nline two\n");

    let git = Git::system();
    let working = git
        .diff(
            Some(repo.path()),
            &DiffRequest::working_tree(Some("file.txt".into())),
        )
        .expect("working tree diff");
    assert_eq!(working.target, DiffTarget::WorkingTree);
    assert_eq!(working.path.as_deref(), Some("file.txt"));
    assert_eq!(working.base, None);
    assert!(working.patch.contains("-line one"), "{}", working.patch);
    assert!(
        working.patch.contains("+line one changed"),
        "{}",
        working.patch
    );
    assert!(!working.truncated);

    repo.git(&["checkout", "--quiet", "-b", "feature"]);
    repo.write("file.txt", "feature line\n");
    repo.commit_all("feature");
    repo.git(&["checkout", "--quiet", "main"]);

    let against_base = git
        .diff(
            Some(repo.path()),
            &DiffRequest::branch("main", "feature", None),
        )
        .expect("branch diff");
    assert_eq!(against_base.target, DiffTarget::Branch);
    assert_eq!(against_base.base.as_deref(), Some("main"));
    assert_eq!(against_base.branch.as_deref(), Some("feature"));
    assert!(
        against_base.patch.contains("+feature line"),
        "{}",
        against_base.patch
    );
}

#[test]
fn diff_truncates_a_large_patch_with_a_marker_and_a_flag() {
    let repo = Repo::new();
    repo.write("big.txt", "start\n");
    repo.commit_all("init");

    let mut big = String::new();
    for line in 0..40_000 {
        big.push_str(&format!(
            "line {line:05} of a deliberately long file used to prove truncation\n"
        ));
    }
    repo.write("big.txt", &big);
    assert!(big.len() > MAX_OUTPUT_BYTES * 2);

    let diff = Git::system()
        .diff(
            Some(repo.path()),
            &DiffRequest::working_tree(Some("big.txt".into())),
        )
        .expect("large diff");

    assert!(diff.truncated, "a patch over the bound must set truncated");
    assert!(
        diff.patch.contains("output truncated"),
        "{}",
        &diff.patch[diff.patch.len().saturating_sub(200)..]
    );
    assert!(
        diff.patch.len() <= MAX_OUTPUT_BYTES + 200,
        "bounded patch was {} bytes",
        diff.patch.len()
    );
}

#[test]
fn diff_requires_a_branch_and_base_together() {
    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("init");
    let git = Git::system();

    let err = git
        .diff(Some(repo.path()), &DiffRequest::branch("main", "", None))
        .expect_err("empty branch is invalid");
    assert_eq!(err.code, ErrorCode::Invalid);

    let err = git
        .diff(
            Some(repo.path()),
            &DiffRequest::branch("main", "--output=pwned", None),
        )
        .expect_err("option-looking refs are rejected");
    assert_eq!(err.code, ErrorCode::Invalid);
}

// ---------------------------------------------------------------------------------------------
// Commit
// ---------------------------------------------------------------------------------------------

#[test]
fn commit_lands_only_the_selected_paths_and_returns_the_fresh_status() {
    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    repo.write("b.txt", "b\n");
    repo.commit_all("init");
    let before = repo.git(&["rev-parse", "HEAD"]);

    repo.write("a.txt", "a changed\n");
    repo.write("b.txt", "b changed\n");
    repo.write("new.txt", "brand new\n");

    let status = Git::system()
        .commit(
            Some(repo.path()),
            "change a",
            &["a.txt".to_string(), "new.txt".to_string()],
        )
        .expect("commit");

    assert!(repo.git(&["rev-parse", "HEAD"]) != before, "HEAD moved");
    let names = repo.git(&["log", "-1", "--name-only", "--format=%s"]);
    let names: Vec<&str> = names.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(names, vec!["change a", "a.txt", "new.txt"]);
    // The unselected file is still dirty, and the returned status says so.
    assert!(!status.clean);
    assert_eq!(status.files.len(), 1);
    assert_eq!(status.files[0].path, "b.txt");
    assert!(status.head.is_some());
}

#[test]
fn commit_rejects_empty_messages_empty_paths_and_paths_outside_the_repo() {
    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("init");
    repo.write("a.txt", "a changed\n");
    let git = Git::system();

    for (message, paths) in [
        ("   ", vec!["a.txt".to_string()]),
        ("message", Vec::new()),
        ("message", vec!["../outside.txt".to_string()]),
        (
            "message",
            vec!["C:/Windows/System32/drivers/etc/hosts".to_string()],
        ),
    ] {
        let err = git
            .commit(Some(repo.path()), message, &paths)
            .expect_err("invalid commit request");
        assert_eq!(err.code, ErrorCode::Invalid, "{err:?}");
    }
    // Nothing was staged by the rejected calls.
    assert_eq!(repo.git(&["diff", "--cached", "--name-only"]), "");
}

// ---------------------------------------------------------------------------------------------
// Pull requests (fake `gh`)
// ---------------------------------------------------------------------------------------------

fn gh_output(code: i32, stdout: &str, stderr: &str) -> CommandOutput {
    CommandOutput {
        code,
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
    }
}

#[test]
fn open_pr_returns_the_url_and_passes_an_exact_argv() {
    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("init");

    let stub = Arc::new(GhStub::new(Ok(gh_output(
        0,
        "Creating draft pull request for ticket/04-git-panel into main in acme/grove\nhttps://github.com/acme/grove/pull/42\n",
        "",
    ))));
    let git = Git::with_runner(stub.clone());

    let result = git
        .open_pr(
            Some(repo.path()),
            &PrRequest::new("Add the git panel", "Ticket 04.")
                .with_base("main")
                .with_draft(true),
        )
        .expect("draft PR");
    assert_eq!(result.url, "https://github.com/acme/grove/pull/42");
    assert!(result.draft);
    assert_eq!(result.base.as_deref(), Some("main"));

    let specs = stub.gh_specs();
    assert_eq!(specs.len(), 1, "one gh invocation");
    let spec = &specs[0];
    assert_eq!(spec.program, "gh");
    assert_eq!(
        spec.args,
        vec![
            "pr",
            "create",
            "--draft",
            "--title",
            "Add the git panel",
            "--body",
            "Ticket 04.",
            "--base",
            "main",
        ]
    );
    assert_eq!(
        canonical(spec.cwd.as_deref().expect("cwd")),
        canonical(repo.path())
    );
    assert!(
        spec.env
            .iter()
            .any(|(key, value)| key == "GH_PROMPT_DISABLED" && value == "1"),
        "gh must never prompt: {:?}",
        spec.env
    );
}

#[test]
fn open_pr_with_a_missing_gh_is_unavailable_with_the_reason() {
    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("init");

    let git = Git::with_binaries(
        Arc::new(SystemCommandRunner),
        "git",
        "grove-definitely-missing-gh-binary",
    );
    let err = git
        .open_pr(Some(repo.path()), &PrRequest::new("Title", "Body"))
        .expect_err("gh is absent");
    assert_eq!(err.code, ErrorCode::Unavailable);
    assert!(
        err.message.contains("grove-definitely-missing-gh-binary"),
        "{err:?}"
    );
    assert!(!err.message.is_empty());
}

#[test]
fn open_pr_unauthenticated_is_unavailable_with_ghs_reason() {
    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("init");

    let stub = Arc::new(GhStub::new(Ok(gh_output(
        1,
        "",
        "To get started with GitHub CLI, please run:  gh auth login\nAlternatively, populate the GH_TOKEN environment variable with a GitHub API authentication token.\n",
    ))));
    let git = Git::with_runner(stub);
    let err = git
        .open_pr(Some(repo.path()), &PrRequest::new("Title", "Body"))
        .expect_err("gh is unauthenticated");
    assert_eq!(err.code, ErrorCode::Unavailable);
    assert!(err.message.contains("not authenticated"), "{err:?}");
    assert!(
        err.detail
            .as_deref()
            .unwrap_or_default()
            .contains("gh auth login"),
        "the reason must be surfaced: {err:?}"
    );
}

#[test]
fn open_pr_other_gh_failure_is_invalid_with_stderr() {
    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("init");

    let stub = Arc::new(GhStub::new(Ok(gh_output(
        1,
        "",
        "pull request create failed: GraphQL: No commits between main and feature\n",
    ))));
    let git = Git::with_runner(stub);
    let err = git
        .open_pr(Some(repo.path()), &PrRequest::new("Title", "Body"))
        .expect_err("gh refused");
    assert_eq!(err.code, ErrorCode::Invalid);
    assert!(
        err.detail
            .as_deref()
            .unwrap_or_default()
            .contains("No commits between"),
        "{err:?}"
    );
}

#[test]
fn open_pr_rejects_an_empty_title_before_invoking_gh() {
    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("init");

    let stub = Arc::new(GhStub::new(Ok(gh_output(
        0,
        "https://example.test/pr/1\n",
        "",
    ))));
    let git = Git::with_runner(stub.clone());
    let err = git
        .open_pr(Some(repo.path()), &PrRequest::new("  ", "Body"))
        .expect_err("title is required");
    assert_eq!(err.code, ErrorCode::Invalid);
    assert!(
        stub.gh_specs().is_empty(),
        "gh must not run for an invalid request"
    );
}

// ---------------------------------------------------------------------------------------------
// The runner seam and absent binaries
// ---------------------------------------------------------------------------------------------

#[test]
fn every_git_command_crosses_the_runner_with_an_argv_and_the_repo_as_cwd() {
    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("init");
    repo.write("a.txt", "a changed\n");

    let runner = Arc::new(RecordingRunner::new());
    let git = Git::with_runner(runner.clone());
    let _ = git.status(Some(repo.path())).expect("status");
    let _ = git.branches(Some(repo.path())).expect("branches");
    let _ = git.worktrees(Some(repo.path())).expect("worktrees");
    let _ = git
        .diff(Some(repo.path()), &DiffRequest::working_tree(None))
        .expect("diff");
    let _ = git
        .commit(Some(repo.path()), "change", &["a.txt".to_string()])
        .expect("commit");

    let specs = runner.specs();
    assert!(
        specs.len() >= 6,
        "expected a process per command: {specs:?}"
    );
    let root = canonical(repo.path());
    for spec in specs {
        assert_eq!(
            spec.program, "git",
            "gitops must invoke git directly: {spec:?}"
        );
        assert!(!spec.args.is_empty(), "args are a list: {spec:?}");
        assert!(
            !spec
                .args
                .iter()
                .any(|arg| arg == "-c" || arg == "sh" || arg == "cmd"),
            "no shell or inline git config: {spec:?}"
        );
        assert_eq!(
            canonical(spec.cwd.as_deref().expect("cwd")),
            root,
            "{spec:?}"
        );
    }
}

#[test]
fn a_missing_git_binary_is_unavailable_not_a_panic() {
    let repo = Repo::new();
    let git = Git::with_binaries(
        Arc::new(SystemCommandRunner),
        "grove-definitely-missing-git-binary",
        "gh",
    );
    let err = git.status(Some(repo.path())).expect_err("git is absent");
    assert_eq!(err.code, ErrorCode::Unavailable);
    assert!(
        err.message.contains("grove-definitely-missing-git-binary"),
        "{err:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Staging, detection and worktrees (redesigned shell)
// ---------------------------------------------------------------------------------------------

#[test]
fn stage_and_unstage_move_exactly_the_selected_paths() {
    let repo = Repo::new();
    repo.write("tracked.txt", "one\n");
    repo.commit_all("init");
    repo.write("tracked.txt", "two\n");
    repo.write("new.txt", "new\n");
    let git = Git::system();

    let status = git
        .stage(Some(repo.path()), &["tracked.txt".into()], true)
        .expect("stage");
    assert!(find_file(&status, "tracked.txt").staged);
    assert!(find_file(&status, "new.txt").untracked);

    let status = git
        .stage(Some(repo.path()), &["tracked.txt".into()], false)
        .expect("unstage");
    assert!(!find_file(&status, "tracked.txt").staged);

    assert!(git.stage(Some(repo.path()), &[], true).is_err());
    assert!(git
        .stage(Some(repo.path()), &["../outside.txt".into()], true)
        .is_err());
}

#[test]
fn detect_reports_a_repository_with_branches_and_a_plain_folder_as_not_git() {
    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("init");
    repo.git(&["remote", "add", "origin", "git@github.com:acme/grove.git"]);
    let git = Git::system();

    let detection = git.detect(repo.path()).expect("detect");
    assert!(detection.is_git);
    assert_eq!(detection.current_branch.as_deref(), Some("main"));
    assert_eq!(detection.branches, vec!["main".to_string()]);
    assert_eq!(detection.default_branch.as_deref(), Some("main"));
    assert_eq!(detection.remote.as_deref(), Some("acme/grove"));

    let plain = tempfile::tempdir().expect("temp dir");
    let detection = git.detect(plain.path()).expect("detect plain");
    assert!(!detection.is_git);
    assert!(detection.branches.is_empty());
}

#[test]
fn create_worktree_makes_a_new_branch_and_refuses_a_non_empty_folder() {
    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("init");
    let checkout = tempfile::tempdir().expect("checkout parent");
    let target = checkout.path().join("wt-feature");
    let git = Git::system();

    let worktree = git
        .create_worktree(
            Some(repo.path()),
            "feature/ui",
            "main",
            target.to_str().expect("utf-8 path"),
        )
        .expect("worktree");
    assert_eq!(worktree.branch.as_deref(), Some("feature/ui"));
    assert_eq!(canonical(Path::new(&worktree.path)), canonical(&target));
    assert!(target.join("a.txt").is_file());

    let error = git
        .create_worktree(
            Some(repo.path()),
            "feature/other",
            "main",
            target.to_str().expect("utf-8 path"),
        )
        .expect_err("a used folder is refused");
    assert_eq!(error.code, ErrorCode::Invalid);
}

#[test]
fn an_untracked_file_gets_a_new_file_patch_for_the_inspector() {
    let repo = Repo::new();
    repo.write("src/new.rs", "fn main() {\n    println!(\"hi\");\n}\n");
    let git = Git::system();

    let diff = git
        .diff(
            Some(repo.path()),
            &DiffRequest::working_tree(Some("src/new.rs".into())),
        )
        .expect("untracked diff");

    assert_eq!(diff.path.as_deref(), Some("src/new.rs"));
    assert_eq!(diff.target, DiffTarget::WorkingTree);
    assert!(!diff.truncated);
    assert!(
        diff.patch.contains("new file mode 100644"),
        "{}",
        diff.patch
    );
    assert!(diff.patch.contains("+fn main() {"), "{}", diff.patch);
    assert!(
        diff.patch.contains("+    println!(\"hi\");"),
        "{}",
        diff.patch
    );
    assert!(
        !diff.patch.contains("+++ b/src/new.rs\n+"),
        "headers are not content"
    );
}

#[test]
fn a_binary_untracked_file_is_named_not_sprayed() {
    let repo = Repo::new();
    std::fs::write(repo.path().join("image.bin"), [0u8, 1, 2, 3]).expect("write binary");
    let git = Git::system();

    let diff = git
        .diff(
            Some(repo.path()),
            &DiffRequest::working_tree(Some("image.bin".into())),
        )
        .expect("binary diff");
    assert!(
        diff.patch.contains("Binary file image.bin not shown"),
        "{}",
        diff.patch
    );
}
