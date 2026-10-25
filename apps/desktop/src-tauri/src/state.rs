use grove_core::specgraph::SpecWatcher;
use grove_core::GroveCore;
use std::sync::{Arc, Mutex};

/// Tauri-managed state. Every command takes `State<'_, AppState>`; services hang off `core`.
pub struct AppState {
    pub core: Arc<GroveCore>,
    /// Held only for its lifetime: dropping it stops the native watcher and its worker thread.
    spec_watcher: Mutex<Option<SpecWatcher>>,
    /// The window handle, so a command can emit and re-target the watcher without every command
    /// signature carrying a runtime generic.
    app: Mutex<Option<tauri::AppHandle>>,
}

impl AppState {
    pub fn new(core: GroveCore) -> Self {
        Self {
            core: Arc::new(core),
            spec_watcher: Mutex::new(None),
            app: Mutex::new(None),
        }
    }

    /// Hand the state its window. `setup` calls this; a headless construction simply leaves it unset
    /// and anything that would emit is a no-op.
    pub fn set_app_handle(&self, app: tauri::AppHandle) {
        if let Ok(mut slot) = self.app.lock() {
            *slot = Some(app);
        }
    }

    pub fn app_handle(&self) -> Option<tauri::AppHandle> {
        self.app.lock().ok().and_then(|slot| slot.clone())
    }

    /// Whether a watcher is already running, so a re-adoption of the same root does not restart it.
    pub fn has_spec_watcher(&self) -> bool {
        self.spec_watcher
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    /// Keep the spec watcher alive for as long as the app runs.
    pub fn keep_spec_watcher(&self, watcher: SpecWatcher) {
        if let Ok(mut slot) = self.spec_watcher.lock() {
            *slot = Some(watcher);
        }
    }
}
