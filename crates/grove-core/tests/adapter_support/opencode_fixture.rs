//! Start contexts for the scripted OpenCode server (`fixtures/harnesses/opencode_fake.mjs`).
//!
//! OpenCode is the first adapter that is not a stdio child: the fixture is a real (local) HTTP server
//! that serves REST plus an SSE bus, so the tests exercise the adapter's readiness probe, its HTTP
//! commands and its event-stream reader against a process rather than a mock.

#![allow(dead_code)]

use super::conformance::Target;
use super::runtime::fixture;
use grove_core::adapters::{OpenCodeAdapter, StartContext};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;

pub const FIXTURE: &str = "opencode_fake.mjs";

/// `node fixtures/harnesses/opencode_fake.mjs serve --mode <mode> ...` behind the documented
/// `config.command` / `config.args` override. `state_dir` is the fixture's session store, which is
/// what makes a restart resumable (the same directory must be passed to the resumed session).
pub fn config(mode: &str, state_dir: &Path, extra: &[&str]) -> Value {
    let mut args = vec![
        fixture(FIXTURE).to_string_lossy().to_string(),
        "serve".to_string(),
        "--mode".to_string(),
        mode.to_string(),
        "--state-dir".to_string(),
        state_dir.to_string_lossy().to_string(),
    ];
    args.extend(extra.iter().map(|value| (*value).to_string()));
    json!({ "command": "node", "args": args })
}

pub fn context(mode: &str, state_dir: &Path, extra: &[&str]) -> StartContext {
    StartContext {
        cwd: std::env::temp_dir(),
        model: Some("anthropic/claude-sonnet-4".into()),
        prompt: None,
        resume_token: None,
        config: config(mode, state_dir, extra),
        context_files: Vec::new(),
    }
}

/// The conformance scenarios mapped onto fixture modes. `turn` uses the happy mode, which carries
/// text deltas, a tool call, an edit diff and usage — everything the adapter claims.
pub fn target() -> Target {
    let state = Arc::new(tempfile::tempdir().expect("a temp dir for the fixture's session store"));
    Target::new(
        Arc::new(OpenCodeAdapter::default()),
        move |scenario| match scenario {
            "slow" => context("slow", state.path(), &["--delay-ms", "40"]),
            "unknown" => context("unknown", state.path(), &["--delay-ms", "5"]),
            _ => context("happy", state.path(), &["--delay-ms", "5"]),
        },
    )
}
