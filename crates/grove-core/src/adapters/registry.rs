//! The adapter registry — the one place that names every built-in harness.
//!
//! Adding an adapter is one module plus one line here (see `architecture.md` §Extension seams). The
//! list is returned as trait objects so sessions never know a concrete type.

use super::acp::AcpAdapter;
use super::claude::ClaudeAdapter;
use super::mock::MockAdapter;
use super::pi::PiAdapter;
use super::opencode::OpenCodeAdapter;
use super::HarnessAdapter;
use std::sync::Arc;

pub fn builtin_registry() -> Vec<Arc<dyn HarnessAdapter>> {
    vec![
        Arc::new(MockAdapter),
        Arc::new(ClaudeAdapter::default()),
        Arc::new(AcpAdapter::default()),
        Arc::new(PiAdapter::default()),
        Arc::new(OpenCodeAdapter::default()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_adapter_has_a_unique_id() {
        let mut ids: Vec<&'static str> = builtin_registry()
            .iter()
            .map(|adapter| adapter.id().as_str())
            .collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "adapter ids must be unique");
    }
}
