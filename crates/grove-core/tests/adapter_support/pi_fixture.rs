//! Start contexts and the conformance target for the scripted sidecar stand-in
//! (`fixtures/harnesses/pi_sidecar_fake.mjs`).
//!
//! The fixture speaks Grove Sidecar Protocol v1, so no test needs Node's sidecar build, a real `pi`
//! install, or the network: the adapter is pointed at the fixture through `config.command` /
//! `config.args`, exactly like a user points it at a wrapper.

#![allow(dead_code)]

use super::conformance::Target;
use super::runtime::fixture;
use grove_core::adapters::{PiAdapter, StartContext};
use serde_json::{json, Value};
use std::sync::Arc;

pub const FIXTURE: &str = "pi_sidecar_fake.mjs";

/// `node fixtures/harnesses/pi_sidecar_fake.mjs --mode <mode> [extra...]` behind the documented
/// `config.command` / `config.args` override.
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
        model: Some("pi-fake-1".into()),
        prompt: None,
        resume_token: None,
        config: config(mode, extra),
        context_files: Vec::new(),
    }
}

/// The conformance scenarios mapped onto fixture modes. `turn` uses the tool-use mode so the suite
/// checks the whole call/result ordering, not just text.
pub fn target() -> Target {
    Target::new(Arc::new(PiAdapter::default()), |scenario| match scenario {
        "slow" => context("slow", &["--delay-ms", "20"]),
        "unknown" => context("unknown", &[]),
        _ => context("tools", &[]),
    })
}
