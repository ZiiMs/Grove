//! Repository reads and writes: discovery, status, branches, worktrees, commit.
//!
//! Parsing is split from process launching (`parse_status`, `parse_branches`, `parse_worktrees`) so the
//! porcelain formats are unit-tested against literal samples as well as against a real repository.

use std::path::{Path, PathBuf};

use super::{
    git_failure, non_empty, tail, validate_path, validate_ref, Git, GitBranch, GitDetection,
    GitStatus, GitStatusFile, GitWorktree, MAX_OUTPUT_BYTES,
};
use crate::ipc::{IpcError, IpcResult};
use crate::paths::display_path;

/// NUL-separated porcelain v1 records: `-z` is what makes non-ASCII and space-containing paths safe.
const STATUS_ARGS: [&str; 5] = [
    "status",
    "--porcelain=v1",
    "-z",
    "--branch",
    "--untracked-files=all",
];

/// Tabs are field separators because git forbids control characters (and spaces) in ref names.
const BRANCH_FORMAT: &str =
    "--format=%(refname)\t%(refname:short)\t%(upstream:short)\t%(upstream:track)\t%(HEAD)";

impl Git {
    /// Current branch, HEAD commit, and every changed path.
    pub fn status(&self, repo: Option<&Path>) -> IpcResult<GitStatus> {
        let root = self.discover(repo)?;
        let output = self.run(&root, &STATUS_ARGS)?;
        if !output.success() {
            return Err(git_failure("status", &output));
        }
        let (raw, truncated) = bound_status(&output.stdout);
        let (branch, mut files) = parse_status(&raw);
        let head = self.head(&root)?;
        self.attach_line_stats(&root, head.is_some(), &mut files)?;
        Ok(GitStatus {
            repo_root: display_path(&root),
            branch,
            head,
            clean: files.is_empty(),
            truncated,
            files,
        })
    }

    /// Full id of HEAD, `None` before the first commit (an expected state, not an error).
    fn head(&self, root: &Path) -> IpcResult<Option<String>> {
        let output = self.run(root, &["rev-parse", "--verify", "HEAD"])?;
        if output.success() {
            Ok(non_empty(&output.stdout))
        } else {
            Ok(None)
        }
    }

    /// Local branches and remote-tracking branches, with upstream and ahead/behind counts (local
    /// arithmetic only; gitops never fetches).
    pub fn branches(&self, repo: Option<&Path>) -> IpcResult<Vec<GitBranch>> {
        let root = self.discover(repo)?;
        let output = self.run(
            &root,
            &["for-each-ref", BRANCH_FORMAT, "refs/heads", "refs/remotes"],
        )?;
        if !output.success() {
            return Err(git_failure("for-each-ref", &output));
        }
        Ok(parse_branches(&output.stdout))
    }

    /// Every worktree, marking the one the command ran in.
    pub fn worktrees(&self, repo: Option<&Path>) -> IpcResult<Vec<GitWorktree>> {
        let root = self.discover(repo)?;
        let output = self.run(&root, &["worktree", "list", "--porcelain"])?;
        if !output.success() {
            return Err(git_failure("worktree list", &output));
        }
        Ok(parse_worktrees(&output.stdout, &display_path(&root)))
    }

    /// Stage and commit exactly the selected repository-relative paths.
    ///
    /// Paths are staged first so a brand-new file can be committed, then handed to `git commit -- <paths>`
    /// so changes the user did not select are neither committed nor unstaged. Returns the fresh status.
    pub fn commit(
        &self,
        repo: Option<&Path>,
        message: &str,
        paths: &[String],
    ) -> IpcResult<GitStatus> {
        let root = self.discover(repo)?;
        let message = message.trim();
        if message.is_empty() {
            return Err(IpcError::invalid("commit message must not be empty"));
        }
        if paths.is_empty() {
            return Err(IpcError::invalid("select at least one path to commit"));
        }
        for path in paths {
            validate_path(path)?;
        }

        let mut add = vec!["add".to_string(), "--all".to_string(), "--".to_string()];
        add.extend(paths.iter().cloned());
        let staged = self.run_args(&root, &add)?;
        if !staged.success() {
            return Err(git_failure("add", &staged));
        }

        let mut commit_args = vec![
            "commit".to_string(),
            "--message".to_string(),
            message.to_string(),
            "--".to_string(),
        ];
        commit_args.extend(paths.iter().cloned());
        let committed = self.run_args(&root, &commit_args)?;
        if !committed.success() {
            if committed
                .stderr
                .to_lowercase()
                .contains("nothing to commit")
            {
                return Err(
                    IpcError::invalid("no changes to commit for the selected paths")
                        .with_detail(committed.stderr_tail()),
                );
            }
            return Err(git_failure("commit", &committed));
        }

        self.status(Some(&root))
    }

    /// Stage or unstage exactly the selected paths, then return the fresh status.
    ///
    /// Committing already stages its own selection; this is the separate checkbox operation the UI
    /// exposes, so "staged" in the changes list reflects git rather than a client-side set.
    pub fn stage(
        &self,
        repo: Option<&Path>,
        paths: &[String],
        staged: bool,
    ) -> IpcResult<GitStatus> {
        let root = self.discover(repo)?;
        if paths.is_empty() {
            return Err(IpcError::invalid("select at least one path to stage"));
        }
        for path in paths {
            validate_path(path)?;
        }

        let mut args = if staged {
            vec!["add".to_string(), "--all".to_string(), "--".to_string()]
        } else {
            vec![
                "restore".to_string(),
                "--staged".to_string(),
                "--".to_string(),
            ]
        };
        args.extend(paths.iter().cloned());
        let output = self.run_args(&root, &args)?;
        if !output.success() {
            if !staged {
                // Before the first commit `git restore --staged` has no HEAD to restore from; reset
                // is the same operation in that state.
                let mut reset = vec![
                    "reset".to_string(),
                    "-q".to_string(),
                    "HEAD".to_string(),
                    "--".to_string(),
                ];
                reset.extend(paths.iter().cloned());
                let fallback = self.run_args(&root, &reset)?;
                if fallback.success() {
                    return self.status(Some(&root));
                }
            }
            return Err(git_failure(if staged { "add" } else { "restore" }, &output));
        }
        self.status(Some(&root))
    }

    /// Create a branch and a checkout for it, then report the new worktree.
    ///
    /// `path` may be outside the repository (that is the point of a worktree); only the branch and
    /// base refs are validated as git names. An existing non-empty folder is refused rather than
    /// reusing a checkout the user did not choose.
    pub fn create_worktree(
        &self,
        repo: Option<&Path>,
        branch: &str,
        base: &str,
        path: &str,
    ) -> IpcResult<GitWorktree> {
        let root = self.discover(repo)?;
        let branch = validate_ref("branch", branch)?;
        let base = validate_ref("base", base)?;
        let path = path.trim();
        if path.is_empty() {
            return Err(IpcError::invalid("a worktree path is required"));
        }
        let target = PathBuf::from(path);
        if target.exists() {
            let empty = std::fs::read_dir(&target)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false);
            if !empty {
                return Err(IpcError::invalid(format!(
                    "the worktree path already exists and is not empty: {path}"
                )));
            }
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                IpcError::io(format!("cannot create {}: {error}", parent.display()))
            })?;
        }

        let args = vec![
            "worktree".to_string(),
            "add".to_string(),
            "-b".to_string(),
            branch,
            path.to_string(),
            base,
        ];
        let output = self.run_args(&root, &args)?;
        if !output.success() {
            let detail = tail(&output.stderr, 500);
            return Err(IpcError::invalid("could not create the worktree").with_detail(detail));
        }

        let wanted = display_path(&target);
        self.worktrees(Some(&root))?
            .into_iter()
            .find(|worktree| same_path(&worktree.path, &wanted))
            .ok_or_else(|| {
                IpcError::io(format!(
                    "git created the worktree but it is missing from worktree list: {wanted}"
                ))
            })
    }

    /// Remove a linked worktree, optionally deleting its local branch.
    ///
    /// Every refusal happens before `worktree remove`, so a dirty checkout or an unmerged branch
    /// cannot leave a half-deleted worktree. The main checkout is never removable.
    pub fn remove_worktree(
        &self,
        repo: Option<&Path>,
        worktree_path: &str,
        delete_branch: bool,
        force: bool,
        force_branch: bool,
    ) -> IpcResult<()> {
        let root = self.discover(repo)?;
        let path = worktree_path.trim();
        if path.is_empty() {
            return Err(IpcError::invalid("a worktree path is required"));
        }
        let wanted = display_path(Path::new(path));
        let worktrees = self.worktrees(Some(&root))?;
        let entry = worktrees
            .iter()
            .find(|worktree| same_path(&worktree.path, &wanted))
            .ok_or_else(|| {
                IpcError::invalid(format!("not a worktree of this repository: {path}"))
            })?;
        if entry.current {
            return Err(IpcError::invalid(
                "the main checkout cannot be deleted as a worktree",
            ));
        }

        let branch = entry.branch.as_deref();
        if delete_branch {
            let branch = branch.ok_or_else(|| {
                IpcError::invalid(
                    "this worktree has a detached HEAD; there is no local branch to delete",
                )
            })?;
            validate_ref("branch", branch)?;
            if !force_branch {
                let main = self.run(&root, &["symbolic-ref", "--short", "HEAD"])?;
                let main = non_empty(&main.stdout);
                let merged = match main {
                    Some(main) => self
                        .run(&root, &["merge-base", "--is-ancestor", branch, &main])?
                        .success(),
                    None => false,
                };
                if !merged {
                    return Err(IpcError::invalid(format!(
                        "the branch {branch} has commits that are not in the main checkout"
                    ))
                    .with_detail(
                        "tick \"delete the branch even if it is unmerged\" to discard them",
                    ));
                }
            }
        }

        let status = self.run(Path::new(path), &["status", "--porcelain"])?;
        if !status.success() {
            return Err(git_failure("status", &status));
        }
        if !status.stdout.trim().is_empty() && !force {
            return Err(IpcError::invalid("the worktree has uncommitted changes")
                .with_detail("tick \"discard uncommitted changes\" to remove it"));
        }

        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(path);
        let output = self.run(&root, &args)?;
        if !output.success() {
            return Err(IpcError::invalid("could not remove the worktree")
                .with_detail(tail(&output.stderr, 500)));
        }

        if delete_branch {
            let branch = branch.expect("a branch was required above");
            let flag = if force_branch { "-D" } else { "-d" };
            let output = self.run(&root, &["branch", flag, branch])?;
            if !output.success() {
                return Err(IpcError::invalid(format!(
                    "the worktree was removed, but the branch {branch} could not be deleted"
                ))
                .with_detail(tail(&output.stderr, 500)));
            }
        }
        Ok(())
    }

    /// What the add-project form needs before saving: whether this is a repository, its branches,
    /// current/default branch, and the normalized `owner/name` remote (if one exists).
    pub fn detect(&self, path: &Path) -> IpcResult<GitDetection> {
        let metadata = std::fs::metadata(path).map_err(|error| {
            IpcError::invalid(format!("cannot read {}: {error}", path.display()))
        })?;
        if !metadata.is_dir() {
            return Err(IpcError::invalid(format!(
                "{} is not a folder",
                path.display()
            )));
        }

        let probe = self.run(path, &["rev-parse", "--show-toplevel"])?;
        if !probe.success() {
            let stderr = probe.stderr.to_lowercase();
            if stderr.contains("not a git repository") {
                return Ok(GitDetection {
                    is_git: false,
                    current_branch: None,
                    branches: Vec::new(),
                    default_branch: None,
                    remote: None,
                });
            }
            return Err(git_failure("rev-parse", &probe));
        }
        let root = PathBuf::from(probe.stdout.lines().next().unwrap_or_default().trim());
        if root.as_os_str().is_empty() {
            return Ok(GitDetection {
                is_git: false,
                current_branch: None,
                branches: Vec::new(),
                default_branch: None,
                remote: None,
            });
        }

        let current_branch = self.status(Some(&root))?.branch;
        let mut branches: Vec<String> = self
            .branches(Some(&root))?
            .into_iter()
            .filter(|branch| branch.remote.is_none())
            .map(|branch| branch.name)
            .collect();
        branches.sort();

        let remote_output = self.run(&root, &["config", "--get", "remote.origin.url"])?;
        let remote = if remote_output.success() {
            normalize_remote(remote_output.stdout.trim())
        } else {
            None
        };

        let symbolic = self.run(
            &root,
            &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
        )?;
        let default_branch = if symbolic.success() {
            non_empty(&symbolic.stdout).map(|name| {
                name.strip_prefix("origin/")
                    .unwrap_or(name.as_str())
                    .to_string()
            })
        } else {
            None
        }
        .or_else(|| current_branch.clone())
        .or_else(|| branches.first().cloned());

        Ok(GitDetection {
            is_git: true,
            current_branch,
            branches,
            default_branch,
            remote,
        })
    }
}

/// Keep whole NUL records when a pathological status crosses the bound, and flag the cut.
fn bound_status(raw: &str) -> (String, bool) {
    if raw.len() <= MAX_OUTPUT_BYTES {
        return (raw.to_string(), false);
    }
    let mut cut = MAX_OUTPUT_BYTES;
    while cut > 0 && !raw.is_char_boundary(cut) {
        cut -= 1;
    }
    let cut = raw[..cut].rfind('\0').map_or(0, |index| index + 1);
    (raw[..cut].to_string(), true)
}

fn parse_branch_header(header: &str) -> Option<String> {
    if let Some(rest) = header.strip_prefix("No commits yet on ") {
        return Some(rest.trim().to_string()).filter(|branch| !branch.is_empty());
    }
    if let Some(rest) = header.strip_prefix("Initial commit on ") {
        return Some(rest.trim().to_string()).filter(|branch| !branch.is_empty());
    }
    let name = header.split("...").next().unwrap_or(header).trim();
    // Detached HEAD reads `HEAD (no branch)`; there is no branch name to show.
    if name.is_empty() || name == "HEAD" || name.starts_with("HEAD ") {
        None
    } else {
        Some(name.to_string())
    }
}

fn parse_status(raw: &str) -> (Option<String>, Vec<GitStatusFile>) {
    let mut branch = None;
    let mut files = Vec::new();
    let mut records = raw.split('\0');
    while let Some(record) = records.next() {
        if record.is_empty() {
            continue;
        }
        if let Some(header) = record.strip_prefix("## ") {
            branch = parse_branch_header(header);
            continue;
        }
        let bytes = record.as_bytes();
        // `XY <path>`; the two status characters and the separator are ASCII by construction.
        if bytes.len() < 4 {
            continue;
        }
        let index = (bytes[0] as char).to_string();
        let worktree = (bytes[1] as char).to_string();
        let path = record[3..].to_string();
        let from = if matches!(index.as_str(), "R" | "C") {
            records.next().map(str::to_string)
        } else {
            None
        };
        files.push(GitStatusFile {
            path,
            from,
            staged: index != " " && index != "?",
            untracked: index == "?" && worktree == "?",
            index,
            worktree,
            // Filled from one numstat read plus the untracked file read; see `stats.rs`.
            added: 0,
            removed: 0,
        });
    }
    (branch, files)
}

fn parse_branches(raw: &str) -> Vec<GitBranch> {
    raw.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let refname = fields.next()?.trim();
            let name = fields.next()?.trim();
            let upstream = fields.next().unwrap_or_default().trim();
            let track = fields.next().unwrap_or_default().trim();
            let head = fields.next().unwrap_or_default().trim();
            // `refs/remotes/<remote>/HEAD` is a symbolic alias, not a branch of its own.
            if name.is_empty() || refname.ends_with("/HEAD") {
                return None;
            }
            let (ahead, behind) = parse_track(track);
            Some(GitBranch {
                name: name.to_string(),
                remote: refname
                    .strip_prefix("refs/remotes/")
                    .and_then(|rest| rest.split('/').next())
                    .map(str::to_string),
                current: head == "*",
                upstream: non_empty(upstream),
                ahead,
                behind,
            })
        })
        .collect()
}

/// Parse `%(upstream:track)`: `""`, `"[ahead 1]"`, `"[behind 2]"`, `"[ahead 1, behind 2]"`, `"[gone]"`.
fn parse_track(track: &str) -> (u32, u32) {
    let mut ahead = 0;
    let mut behind = 0;
    for part in track
        .trim_matches(|character| character == '[' || character == ']')
        .split(',')
    {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("ahead ") {
            ahead = value.trim().parse().unwrap_or(0);
        } else if let Some(value) = part.strip_prefix("behind ") {
            behind = value.trim().parse().unwrap_or(0);
        }
    }
    (ahead, behind)
}

fn parse_worktrees(raw: &str, root: &str) -> Vec<GitWorktree> {
    let mut worktrees = Vec::new();
    let mut current: Option<GitWorktree> = None;

    let flush = |worktree: Option<GitWorktree>, worktrees: &mut Vec<GitWorktree>| {
        if let Some(mut worktree) = worktree {
            worktree.current = same_path(&worktree.path, root);
            worktrees.push(worktree);
        }
    };

    for line in raw.lines() {
        if line.is_empty() {
            flush(current.take(), &mut worktrees);
            continue;
        }
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "worktree" => {
                flush(current.take(), &mut worktrees);
                current = Some(GitWorktree {
                    path: value.to_string(),
                    head: None,
                    branch: None,
                    bare: false,
                    detached: false,
                    current: false,
                });
            }
            "HEAD" => {
                if let Some(worktree) = current.as_mut() {
                    worktree.head = non_empty(value);
                }
            }
            "branch" => {
                if let Some(worktree) = current.as_mut() {
                    worktree.branch = Some(
                        value
                            .strip_prefix("refs/heads/")
                            .unwrap_or(value)
                            .to_string(),
                    );
                }
            }
            "bare" => {
                if let Some(worktree) = current.as_mut() {
                    worktree.bare = true;
                }
            }
            "detached" => {
                if let Some(worktree) = current.as_mut() {
                    worktree.detached = true;
                }
            }
            // `locked`/`prunable` carry reasons the DTO does not model yet.
            _ => {}
        }
    }
    flush(current.take(), &mut worktrees);
    worktrees
}

fn normalize_remote(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let without_git = raw.strip_suffix(".git").unwrap_or(raw);
    let tail = if let Some(rest) = without_git.split_once(':').map(|(_, rest)| rest) {
        if !without_git.contains("://") && !without_git.starts_with('/') {
            rest
        } else {
            without_git
        }
    } else {
        without_git
    };
    let cleaned = tail.trim_start_matches('/');
    let mut parts = cleaned.rsplit('/');
    let repo = parts.next().unwrap_or_default();
    let owner = parts.next().unwrap_or_default();
    if owner.is_empty() || repo.is_empty() {
        return non_empty(without_git);
    }
    Some(format!("{owner}/{repo}"))
}

fn same_path(left: &str, right: &str) -> bool {
    // Windows may hand back the 8.3 form (`RUNNER~1`) for a path git reports in long form. Resolve
    // both sides first; a path that does not exist falls back to its literal spelling.
    let normalise = |value: &str| {
        std::fs::canonicalize(value)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|_| value.to_string())
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_string()
    };
    let (left, right) = (normalise(left), normalise(right));
    if cfg!(windows) {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_path_accepts_canonical_and_display_forms() {
        let dir = tempfile::tempdir().expect("temp dir");
        let canonical = std::fs::canonicalize(dir.path()).expect("canonical path");
        assert!(same_path(
            &display_path(dir.path()),
            &display_path(&canonical)
        ));
    }

    #[test]
    fn branch_header_handles_upstreams_empty_repos_and_detached_head() {
        assert_eq!(
            parse_branch_header("main...origin/main [ahead 2, behind 1]").as_deref(),
            Some("main")
        );
        assert_eq!(parse_branch_header("main").as_deref(), Some("main"));
        assert_eq!(
            parse_branch_header("No commits yet on main").as_deref(),
            Some("main")
        );
        assert_eq!(
            parse_branch_header("Initial commit on trunk").as_deref(),
            Some("trunk")
        );
        assert_eq!(parse_branch_header("HEAD (no branch)"), None);
    }

    #[test]
    fn status_parsing_reads_staged_unstaged_untracked_and_renames() {
        let raw = concat!(
            "## main...origin/main [ahead 1]\0",
            "M  staged.txt\0",
            " M worktree.txt\0",
            "?? new.txt\0",
            "R  renamed.txt\0old.txt\0",
            "D  gone.txt\0",
        );
        let (branch, files) = parse_status(raw);
        assert_eq!(branch.as_deref(), Some("main"));
        assert_eq!(files.len(), 5);

        let staged = &files[0];
        assert_eq!(staged.path, "staged.txt");
        assert!(staged.staged && !staged.untracked);
        assert_eq!(
            (staged.index.as_str(), staged.worktree.as_str()),
            ("M", " ")
        );

        let unstaged = &files[1];
        assert!(!unstaged.staged);
        assert_eq!(
            (unstaged.index.as_str(), unstaged.worktree.as_str()),
            (" ", "M")
        );

        let untracked = &files[2];
        assert!(untracked.untracked);
        assert_eq!(untracked.index, "?");

        let renamed = &files[3];
        assert_eq!(renamed.from.as_deref(), Some("old.txt"));
        assert_eq!(renamed.index, "R");

        assert_eq!(files[4].index, "D");
    }

    #[test]
    fn status_parsing_copes_with_a_truncated_document() {
        let raw = "## main\0 M a.txt\0?? b";
        let (branch, files) = parse_status(raw);
        assert_eq!(branch.as_deref(), Some("main"));
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn branch_parsing_reads_remotes_tracking_and_current() {
        let raw = concat!(
            "refs/heads/main\tmain\torigin/main\t[ahead 1, behind 2]\t*\n",
            "refs/heads/local\tlocal\t\t\t \n",
            "refs/remotes/origin/main\torigin/main\t\t\t \n",
            "refs/remotes/origin/HEAD\torigin/HEAD\t\t\t \n",
        );
        let branches = parse_branches(raw);
        assert_eq!(branches.len(), 3, "the remote HEAD alias is skipped");

        assert_eq!(branches[0].name, "main");
        assert_eq!(branches[0].remote, None);
        assert!(branches[0].current);
        assert_eq!(branches[0].upstream.as_deref(), Some("origin/main"));
        assert_eq!((branches[0].ahead, branches[0].behind), (1, 2));

        assert_eq!(branches[1].name, "local");
        assert!(!branches[1].current);
        assert_eq!(branches[1].upstream, None);

        assert_eq!(branches[2].name, "origin/main");
        assert_eq!(branches[2].remote.as_deref(), Some("origin"));
        assert_eq!((branches[2].ahead, branches[2].behind), (0, 0));
    }

    #[test]
    fn track_parsing_covers_every_shape_git_prints() {
        assert_eq!(parse_track(""), (0, 0));
        assert_eq!(parse_track("[ahead 3]"), (3, 0));
        assert_eq!(parse_track("[behind 4]"), (0, 4));
        assert_eq!(parse_track("[ahead 3, behind 4]"), (3, 4));
        assert_eq!(parse_track("[gone]"), (0, 0));
    }

    #[test]
    fn worktree_parsing_marks_the_current_worktree() {
        let raw = concat!(
            "worktree C:/work/grove\n",
            "HEAD 1111111111111111111111111111111111111111\n",
            "branch refs/heads/main\n",
            "\n",
            "worktree C:/work/grove-wt\n",
            "HEAD 2222222222222222222222222222222222222222\n",
            "branch refs/heads/feature\n",
            "\n",
            "worktree C:/work/detached\n",
            "HEAD 3333333333333333333333333333333333333333\n",
            "detached\n",
        );
        let worktrees = parse_worktrees(raw, "C:/work/grove-wt");
        assert_eq!(worktrees.len(), 3);
        assert!(!worktrees[0].current);
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
        assert!(worktrees[1].current);
        assert_eq!(worktrees[1].branch.as_deref(), Some("feature"));
        assert!(worktrees[2].detached);
        assert_eq!(worktrees[2].branch, None);
        assert_eq!(
            worktrees[2].head.as_deref(),
            Some("3333333333333333333333333333333333333333")
        );
    }

    #[test]
    fn status_bounding_keeps_whole_records() {
        let mut raw = String::from("## main\0");
        while raw.len() < MAX_OUTPUT_BYTES + 100 {
            raw.push_str(" M some/long/path.txt\0");
        }
        let (bounded, truncated) = bound_status(&raw);
        assert!(truncated);
        assert!(bounded.ends_with('\0'));
        assert!(bounded.len() <= MAX_OUTPUT_BYTES);
    }

    #[test]
    fn truncate_output_is_the_shared_bound() {
        let (text, truncated) = super::super::truncate_output("0123456789", 4);
        assert!(truncated);
        assert!(text.starts_with("0123"));
    }
}
