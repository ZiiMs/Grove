//! Plumbing every adapter test needs: dependencies, the fixtures directory, and event draining.
//!
//! The adapter tests never touch the network or a real harness install: a fixture script (or the
//! in-process mock) is pointed at through `StartContext.config.command` and `config.args`.

#![allow(dead_code)]

use grove_core::adapters::AdapterDeps;
use grove_core::paths::AppPaths;
use grove_core::secrets::MemorySecretStore;
use grove_core::sessions::event::{EventKind, HarnessEvent, SessionState};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;

/// Generous enough for Node startup on a cold Windows CI runner, short enough to fail a hang.
pub const WAIT: Duration = Duration::from_secs(30);

pub fn deps() -> AdapterDeps {
    AdapterDeps {
        paths: AppPaths::new(std::env::temp_dir().join("grove-adapter-tests")),
        http: reqwest::Client::new(),
        secrets: Arc::new(MemorySecretStore::new()),
    }
}

/// `fixtures/harnesses/<name>` at the repo root.
pub fn fixture(name: &str) -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/harnesses")
        .join(name)
}

pub fn is_state(event: &HarnessEvent, state: SessionState) -> bool {
    matches!(&event.kind, EventKind::Status { state: current } if *current == state)
}

pub async fn next_event(rx: &mut mpsc::Receiver<HarnessEvent>) -> HarnessEvent {
    timeout(WAIT, rx.recv())
        .await
        .expect("the adapter produced an event in time")
        .expect("the adapter session is still running")
}

/// Drain events until `done` matches, returning everything read (the match included).
pub async fn drain_until(
    rx: &mut mpsc::Receiver<HarnessEvent>,
    done: impl Fn(&HarnessEvent) -> bool,
) -> Vec<HarnessEvent> {
    let mut events = Vec::new();
    loop {
        let event = next_event(rx).await;
        let stop = done(&event);
        events.push(event);
        if stop {
            return events;
        }
    }
}

pub async fn drain_until_state(
    rx: &mut mpsc::Receiver<HarnessEvent>,
    state: SessionState,
) -> Vec<HarnessEvent> {
    drain_until(rx, |event| is_state(event, state)).await
}

pub fn tags(events: &[HarnessEvent]) -> Vec<&'static str> {
    events.iter().map(|event| event.kind.tag()).collect()
}
