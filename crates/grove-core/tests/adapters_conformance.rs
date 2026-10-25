//! Conformance: the same expectations run against every adapter the registry knows.
//!
//! The suite itself lives in `adapter_support::conformance`; this file is the list of targets and the
//! guard that keeps the list honest. Adding a harness means adding one target here — an adapter that
//! is registered without one fails `every_registered_adapter_has_a_conformance_target`.

mod adapter_support;

use adapter_support::acp_fixture;
use adapter_support::claude_fixture;
use adapter_support::codex_fixture;
use adapter_support::conformance::{run_all, Target};
use adapter_support::pi_fixture;
use adapter_support::opencode_fixture;
use grove_core::adapters::mock::MockAdapter;
use grove_core::adapters::registry::builtin_registry;
use grove_core::adapters::StartContext;
use std::sync::Arc;

/// The mock ignores the scenario name: it has no transport to script, and its chunk delay is the
/// only knob that matters (a slow stream for the cancel check).
fn mock_target() -> Target {
    Target::new(Arc::new(MockAdapter), |scenario| StartContext {
        cwd: std::env::temp_dir(),
        model: Some("mock-1".into()),
        prompt: None,
        resume_token: None,
        config: serde_json::json!({
            "chunkDelayMs": if scenario == "slow" { 50 } else { 1 },
        }),
        context_files: Vec::new(),
    })
}

#[tokio::test]
async fn mock_passes_conformance() {
    run_all(&mock_target()).await;
}

#[tokio::test]
async fn claude_passes_conformance() {
    run_all(&claude_fixture::target()).await;
}

#[tokio::test]
async fn acp_passes_conformance() {
    run_all(&acp_fixture::target()).await;
}

#[tokio::test]
async fn pi_passes_conformance() {
    run_all(&pi_fixture::target()).await;
}

#[tokio::test]
async fn opencode_passes_conformance() {
    run_all(&opencode_fixture::target()).await;
}

#[tokio::test]
async fn codex_passes_conformance() {
    run_all(&codex_fixture::target()).await;
}

#[test]
fn every_registered_adapter_has_a_conformance_target() {
    let targets = [
        mock_target(),
        claude_fixture::target(),
        acp_fixture::target(),
        pi_fixture::target(),
        opencode_fixture::target(),
        codex_fixture::target(),
    ];
    for adapter in builtin_registry() {
        let name = adapter.id().as_str();
        assert!(
            targets.iter().any(|target| target.name == name),
            "adapter `{name}` is registered but has no conformance target: add one to \
             tests/adapters_conformance.rs"
        );
    }
    let registry = builtin_registry();
    for target in &targets {
        assert!(
            registry
                .iter()
                .any(|adapter| adapter.id().as_str() == target.name),
            "conformance target `{}` is not registered in builtin_registry()",
            target.name
        );
    }
}
