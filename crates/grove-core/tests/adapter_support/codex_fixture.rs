//! Start contexts for the scripted Codex app-server stand-in (`fixtures/harnesses/codex_fake.mjs`).

use super::conformance::Target;
use super::runtime::fixture;
use grove_core::adapters::{CodexAdapter, StartContext};
use serde_json::{json, Value};
use std::sync::Arc;

pub const FIXTURE: &str = "codex_fake.mjs";

/// `node fixtures/harnesses/codex_fake.mjs --mode <mode> [extra...]` behind the documented
/// `config.command` / `config.args` override. The adapter appends the `app-server` subcommand itself,
/// so the fixture sees the real launch form — no test ever needs a `codex` install.
pub fn config(mode: &str, extra: &[&str]) -> Value {
    let mut args = vec![
        fixture(FIXTURE).to_string_lossy().to_string(),
        "--mode".to_string(),
        mode.to_string(),
    ];
    args.extend(extra.iter().map(|value| (*value).to_string()));
    json!({ "command": "node", "args": args })
}

pub fn context(mode: &str, extra: &[&str]) -> StartContext {
    StartContext {
        cwd: std::env::temp_dir(),
        model: Some("codex-fake-1".into()),
        prompt: None,
        resume_token: None,
        config: config(mode, extra),
        context_files: Vec::new(),
    }
}

/// The conformance scenarios mapped onto fixture modes. `turn` is the full happy path (a command
/// execution, a streamed answer, an aggregated diff and usage) so the suite can check every event kind
/// the adapter claims, and `permission` scripts the approval roundtrip.
pub fn target() -> Target {
    Target::new(
        Arc::new(CodexAdapter::default()),
        |scenario| match scenario {
            "slow" => context("slow", &["--delay-ms", "50"]),
            "unknown" => context("unknown-line", &[]),
            "permission" => context("approval", &[]),
            _ => context("happy", &[]),
        },
    )
    .with_permissions()
}
