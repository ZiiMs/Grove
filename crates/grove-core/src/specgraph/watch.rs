//! Filesystem watcher for the spec graph.
//!
//! `notify` is wrapped behind [`SpecWatcher`] so the rest of the core only sees a callback with the
//! relative paths that changed. Events are filtered (markdown, inside the root, not under a skipped
//! directory) and coalesced: an editor save usually produces several raw events, but the shell emits
//! one `grove://spec-changed`.
//!
//! [`SpecWatcher::handle_event`] is the deterministic seam tests use — it runs exactly the filtering
//! and callback a real event would, without waiting on an OS notification. One polling test in
//! `tests/specgraph.rs` covers the native backend with a generous timeout.

use super::index::SKIPPED_DIRS;
use notify::Watcher;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

/// Debounce window for real filesystem events.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(150);

/// Event payload: the spec files that changed, relative to the project root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecChange {
    pub paths: Vec<String>,
}

type ChangeSink = Arc<dyn Fn(SpecChange) + Send + Sync>;

/// Watches one project root and calls `sink` when spec files change.
///
/// Dropping the value stops the native watcher and lets its worker thread exit. The struct is behind
/// a `Mutex` in the Tauri shell because `notify`'s watcher is `Send` but not `Sync`.
pub struct SpecWatcher {
    emitter: Arc<Emitter>,
    runtime: Option<Runtime>,
}

struct Runtime {
    _watcher: notify::RecommendedWatcher,
    _worker: std::thread::JoinHandle<()>,
}

impl std::fmt::Debug for SpecWatcher {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SpecWatcher")
            .field("root", &self.emitter.root)
            .field("running", &self.runtime.is_some())
            .finish()
    }
}

impl SpecWatcher {
    /// Build a watcher value without touching the filesystem. Used by tests that call
    /// [`SpecWatcher::handle_event`] directly.
    pub fn new(root: impl AsRef<Path>, sink: impl Fn(SpecChange) + Send + Sync + 'static) -> Self {
        Self {
            emitter: Arc::new(Emitter::new(root.as_ref().to_path_buf(), Arc::new(sink))),
            runtime: None,
        }
    }

    /// Start the native watcher with the default debounce window.
    pub fn start(
        root: impl AsRef<Path>,
        sink: impl Fn(SpecChange) + Send + Sync + 'static,
    ) -> notify::Result<Self> {
        Self::start_with_debounce(root, sink, DEFAULT_DEBOUNCE)
    }

    /// Start the native watcher, coalescing raw events that arrive within `debounce`.
    pub fn start_with_debounce(
        root: impl AsRef<Path>,
        sink: impl Fn(SpecChange) + Send + Sync + 'static,
        debounce: Duration,
    ) -> notify::Result<Self> {
        let emitter = Arc::new(Emitter::new(root.as_ref().to_path_buf(), Arc::new(sink)));
        let watched_root = emitter.root.clone();
        let (sender, receiver) = mpsc::channel::<Vec<String>>();

        let callback_emitter = Arc::clone(&emitter);
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else { return };
                let paths = callback_emitter.paths(&event);
                if !paths.is_empty() {
                    let _ = sender.send(paths);
                }
            })?;
        watcher.watch(&watched_root, notify::RecursiveMode::Recursive)?;

        let worker_emitter = Arc::clone(&emitter);
        let worker = std::thread::Builder::new()
            .name("grove-spec-watch".to_string())
            .spawn(move || debounce_loop(receiver, worker_emitter, debounce))
            .map_err(notify::Error::io)?;

        Ok(Self {
            emitter,
            runtime: Some(Runtime {
                _watcher: watcher,
                _worker: worker,
            }),
        })
    }

    /// The indexed root this watcher reports against.
    pub fn root(&self) -> &Path {
        &self.emitter.root
    }

    /// Filter one raw event and invoke the sink synchronously.
    ///
    /// Public so tests (and the debounce worker) can exercise the callback without timing.
    pub fn handle_event(&self, event: &notify::Event) {
        self.emitter.emit(self.emitter.paths(event));
    }
}

fn debounce_loop(receiver: mpsc::Receiver<Vec<String>>, emitter: Arc<Emitter>, debounce: Duration) {
    while let Ok(mut batch) = receiver.recv() {
        loop {
            match receiver.recv_timeout(debounce) {
                Ok(more) => batch.extend(more),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    emitter.emit(batch);
                    return;
                }
            }
        }
        emitter.emit(batch);
    }
}

struct Emitter {
    root: PathBuf,
    sink: ChangeSink,
}

impl Emitter {
    fn new(root: PathBuf, sink: ChangeSink) -> Self {
        Self {
            root: normalise(&root),
            sink,
        }
    }

    fn paths(&self, event: &notify::Event) -> Vec<String> {
        event
            .paths
            .iter()
            .filter(|path| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| extension.eq_ignore_ascii_case("md"))
                    .unwrap_or(false)
            })
            .filter_map(|path| {
                let path = normalise(path);
                let relative = path.strip_prefix(&self.root).ok()?;
                let parts: Vec<String> = relative
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy().to_string())
                    .collect();
                let (file_name, directories) = parts.split_last()?;
                if directories
                    .iter()
                    .any(|part| SKIPPED_DIRS.contains(&part.to_ascii_lowercase().as_str()))
                {
                    return None;
                }
                let mut relative = directories.join("/");
                if !relative.is_empty() {
                    relative.push('/');
                }
                relative.push_str(file_name);
                Some(relative)
            })
            .collect()
    }

    fn emit(&self, mut paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        paths.sort();
        paths.dedup();
        (self.sink)(SpecChange { paths });
    }
}

/// Canonicalise when possible and drop Windows' verbatim prefix so a watched root and event paths
/// compare cleanly.
fn normalise(path: &Path) -> PathBuf {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    #[cfg(windows)]
    {
        canonical
            .strip_prefix(Path::new(r"\\?\"))
            .unwrap_or(&canonical)
            .to_path_buf()
    }
    #[cfg(not(windows))]
    {
        canonical
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_non_markdown_skipped_directories_and_outside_paths() {
        let root = tempfile::tempdir().unwrap();
        let root = normalise(root.path());
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = {
            let seen = Arc::clone(&seen);
            move |change: SpecChange| seen.lock().unwrap().push(change)
        };
        let watcher = SpecWatcher::new(&root, sink);
        let event = notify::Event::new(notify::EventKind::Any)
            .add_path(root.join("architecture.md"))
            .add_path(root.join("nested/module.md"))
            .add_path(root.join("target/skip.md"))
            .add_path(root.join("notes.txt"))
            .add_path(PathBuf::from("C:/somewhere/else.md"));
        watcher.handle_event(&event);

        assert_eq!(
            *seen.lock().unwrap(),
            vec![SpecChange {
                paths: vec![
                    "architecture.md".to_string(),
                    "nested/module.md".to_string()
                ]
            }]
        );
    }

    #[test]
    fn debounce_loop_coalesces_a_burst_into_one_callback() {
        let (sender, receiver) = mpsc::channel::<Vec<String>>();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = {
            let seen = Arc::clone(&seen);
            move |change: SpecChange| seen.lock().unwrap().push(change)
        };
        let emitter = Arc::new(Emitter::new(PathBuf::from("C:/demo"), Arc::new(sink)));
        sender.send(vec!["a.md".to_string()]).unwrap();
        sender.send(vec!["b.md".to_string()]).unwrap();
        drop(sender);

        debounce_loop(receiver, emitter, Duration::from_millis(50));

        assert_eq!(
            *seen.lock().unwrap(),
            vec![SpecChange {
                paths: vec!["a.md".to_string(), "b.md".to_string()]
            }]
        );
    }

    #[test]
    fn empty_events_do_not_call_the_sink() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = {
            let seen = Arc::clone(&seen);
            move |change: SpecChange| seen.lock().unwrap().push(change)
        };
        let watcher = SpecWatcher::new(PathBuf::from("C:/demo"), sink);
        watcher.handle_event(&notify::Event::new(notify::EventKind::Any));
        assert!(seen.lock().unwrap().is_empty());
    }
}
