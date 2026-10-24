//! Start contexts for the scripted Claude Code stand-in (`fixtures/harnesses/claude_fake.mjs`).

#![allow(dead_code)]

use super::conformance::Target;
use super::runtime::fixture;
use grove_core::adapters::{ClaudeAdapter, StartContext};
use serde_json::{json, Value};
use std::sync::Arc;

pub const FIXTURE: &str = "claude_fake.mjs";

/// `node fixtures/harnesses/claude_fake.mjs --mode <mode> [extra...]` behind the documented
/// `config.command` / `config.args` override — no test ever needs a real `claude` install.
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
        model: Some("claude-fake-1".into()),
        prompt: None,
        resume_token: None,
        config: config(mode, extra),
        context_files: Vec::new(),
    }
}

/// The conformance scenarios mapped onto fixture modes. `turn` uses the tool-use mode so the suite
/// checks the whole call/result ordering, not just text.
pub fn target() -> Target {
    Target::new(
        Arc::new(ClaudeAdapter::default()),
        |scenario| match scenario {
            "slow" => context("slow", &["--delay-ms", "50"]),
            "unknown" => context("unknown", &[]),
            _ => context("tools", &[]),
        },
    )
}
