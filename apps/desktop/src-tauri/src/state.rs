use grove_core::GroveCore;
use std::sync::Arc;

/// Tauri-managed state. Every command takes `State<'_, AppState>`; services hang off `core`.
pub struct AppState {
    pub core: Arc<GroveCore>,
}

impl AppState {
    pub fn new(core: GroveCore) -> Self {
        Self {
            core: Arc::new(core),
        }
    }
}
