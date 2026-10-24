use grove_core::specgraph::SpecWatcher;
use grove_core::GroveCore;
use std::sync::{Arc, Mutex};

/// Tauri-managed state. Every command takes `State<'_, AppState>`; services hang off `core`.
pub struct AppState {
    pub core: Arc<GroveCore>,
    /// Held only for its lifetime: dropping it stops the native watcher and its worker thread.
    spec_watcher: Mutex<Option<SpecWatcher>>,
}

impl AppState {
    pub fn new(core: GroveCore) -> Self {
        Self {
            core: Arc::new(core),
            spec_watcher: Mutex::new(None),
        }
    }

    /// Keep the spec watcher alive for as long as the app runs.
    pub fn keep_spec_watcher(&self, watcher: SpecWatcher) {
        if let Ok(mut slot) = self.spec_watcher.lock() {
            *slot = Some(watcher);
        }
    }
}
