//! Start contexts for the scripted ACP agent stand-in (`fixtures/harnesses/acp_fake.mjs`).

#![allow(dead_code)]

use super::conformance::Target;
use super::runtime::fixture;
use grove_core::adapters::{AcpAdapter, StartContext};
use serde_json::{json, Value};
use std::sync::Arc;

pub const FIXTURE: &str = "acp_fake.mjs";

/// `node fixtures/harnesses/acp_fake.mjs --mode <mode> [extra...]` behind the documented
/// `config.command` / `config.args` override — no test ever needs a real ACP agent installed.
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
        model: Some("acp-fake-1".into()),
        prompt: None,
        resume_token: None,
        config: config(mode, extra),
        context_files: Vec::new(),
    }
}

/// The conformance scenarios mapped onto fixture modes. `turn` needs text chunks and a
/// call/result pair; `slow` is the cancel mode, which also leaves a permission request pending so
/// the suite can prove cancellations answer it.
pub fn target() -> Target {
    Target::new(Arc::new(AcpAdapter::default()), |scenario| match scenario {
        "slow" => context("cancel", &["--delay-ms", "50"]),
        "unknown" => context("unknown-line", &[]),
        "permission" => context("permission", &[]),
        _ => context("happy", &[]),
    })
    .with_permissions()
}
