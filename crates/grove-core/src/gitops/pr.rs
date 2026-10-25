//! Draft pull requests through the real `gh` CLI.
//!
//! `gh` is spawned exactly like `git`: a program name and an argument list through the injectable
//! runner. A missing binary or a logged-out CLI is `IpcError::unavailable` with the reason attached,
//! never a panic and never a silent no-op.

use std::path::Path;

use super::{non_empty, tail, validate_ref, CommandOutput, CommandSpec, Git, PrRequest, PrResult};
use crate::ipc::{IpcError, IpcResult};

impl Git {
    /// `gh pr create`, returning the URL gh prints on success. `request.draft` decides whether
    /// `--draft` is passed; a ready PR is the default in the redesigned dialog.
    pub fn open_pr(&self, repo: Option<&Path>, request: &PrRequest) -> IpcResult<PrResult> {
        let root = self.discover(repo)?;
        let title = non_empty(&request.title)
            .ok_or_else(|| IpcError::invalid("pull request title must not be empty"))?;
        let base = match non_empty(request.base.as_deref().unwrap_or_default()) {
            Some(base) => Some(validate_ref("base", &base)?),
            None => None,
        };
        let head = match non_empty(request.head.as_deref().unwrap_or_default()) {
            Some(head) => Some(validate_ref("head", &head)?),
            None => None,
        };

        let mut args = vec![
            "pr".to_string(),
            "create".to_string(),
            "--title".to_string(),
            title,
            "--body".to_string(),
            request.body.clone(),
        ];
        if request.draft {
            args.insert(2, "--draft".to_string());
        }
        if let Some(base) = &base {
            args.push("--base".to_string());
            args.push(base.clone());
        }
        if let Some(head) = &head {
            args.push("--head".to_string());
            args.push(head.clone());
        }

        let spec = CommandSpec {
            program: self.gh_bin.clone(),
            args,
            cwd: Some(root),
            env: gh_env(),
        };
        let output = self.runner.run(&spec)?;
        if !output.success() {
            return Err(pr_failure(&output));
        }

        let url = output
            .stdout
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with("https://") || line.starts_with("http://"));
        match url {
            Some(url) => Ok(PrResult {
                url: url.to_string(),
                draft: request.draft,
                base,
            }),
            None => Err(
                IpcError::invalid("gh pr create did not report a pull request URL")
                    .with_detail(tail(&format!("{}\n{}", output.stdout, output.stderr), 500)),
            ),
        }
    }
}

fn gh_env() -> Vec<(String, String)> {
    vec![
        // gh must fail with a reason, not block the command waiting for a terminal prompt.
        ("GH_PROMPT_DISABLED".to_string(), "1".to_string()),
        ("GH_NO_UPDATE_NOTIFIER".to_string(), "1".to_string()),
        ("NO_COLOR".to_string(), "1".to_string()),
    ]
}

fn pr_failure(output: &CommandOutput) -> IpcError {
    let reason = tail(&format!("{}\n{}", output.stderr, output.stdout), 500);
    let lower = reason.to_lowercase();
    let unauthenticated = [
        "auth login",
        "not logged in",
        "gh_token",
        "authentication",
        "authenticate",
    ]
    .iter()
    .any(|needle| lower.contains(needle));

    if unauthenticated {
        IpcError::unavailable("gh is not authenticated; run `gh auth login`").with_detail(reason)
    } else {
        IpcError::invalid("gh pr create failed").with_detail(reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_failures_are_unavailable_and_other_failures_invalid() {
        let logged_out = CommandOutput {
            code: 1,
            stdout: String::new(),
            stderr: "To get started with GitHub CLI, please run:  gh auth login".into(),
        };
        let err = pr_failure(&logged_out);
        assert_eq!(err.code, crate::ipc::ErrorCode::Unavailable);
        assert!(err.message.contains("not authenticated"));

        let refused = CommandOutput {
            code: 1,
            stdout: String::new(),
            stderr: "pull request create failed: GraphQL: No commits between main and x".into(),
        };
        let err = pr_failure(&refused);
        assert_eq!(err.code, crate::ipc::ErrorCode::Invalid);
        assert!(err
            .detail
            .unwrap_or_default()
            .contains("No commits between"));
    }
}
