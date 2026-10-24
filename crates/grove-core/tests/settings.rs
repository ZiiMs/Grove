//! Settings persistence and the secret-free guarantee.
//!
//! The last test is the audit the ticket asks for: a sentinel token goes through the secret store and a
//! settings write, then the SQLite bytes (database plus WAL) and every captured log line are searched
//! for it. It must never appear.

use grove_core::paths::AppPaths;
use grove_core::secrets::{MemorySecretStore, SecretStatus, SecretStore, GITHUB_TOKEN};
use grove_core::settings::{Settings, SettingsPatch, SettingsService, Theme};
use grove_core::store::Store;
use grove_core::GroveCore;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

#[test]
fn settings_survive_reopening_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());

    {
        let store = Arc::new(Store::open(&paths.database).unwrap());
        let settings = SettingsService::new(store);
        settings
            .set(SettingsPatch {
                harness_paths: Some(BTreeMap::from([(
                    "claude".to_string(),
                    "C:/Tools/claude.exe".to_string(),
                )])),
                catalog_sources: Some(vec!["acme/skills".to_string()]),
                theme: Some(Theme::Dark),
                ..Default::default()
            })
            .unwrap();
    }

    let store = Arc::new(Store::open(&paths.database).unwrap());
    let settings = SettingsService::new(store);
    let current = settings.get().unwrap();
    assert_eq!(current.theme, Theme::Dark);
    assert_eq!(current.catalog_sources, vec!["acme/skills".to_string()]);
    assert_eq!(
        current.harness_paths.get("claude").map(String::as_str),
        Some("C:/Tools/claude.exe")
    );

    // A partial patch keeps the rest, including across the restart.
    let current = settings
        .set(SettingsPatch {
            theme: Some(Theme::Light),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(current.theme, Theme::Light);
    assert_eq!(current.catalog_sources, vec!["acme/skills".to_string()]);
    assert_eq!(current.harness_paths.len(), 1);
}

#[test]
fn grove_core_new_uses_the_memory_store_and_the_in_memory_database() {
    let dir = tempfile::tempdir().unwrap();
    let core = GroveCore::new(AppPaths::new(dir.path()));

    assert_eq!(core.settings.get().unwrap(), Settings::default());

    // A token set through `new` lives only in memory: no keychain prompt, no database file.
    core.secrets.set(GITHUB_TOKEN, "in-memory-token").unwrap();
    assert_eq!(
        core.secrets.get(GITHUB_TOKEN).unwrap().as_deref(),
        Some("in-memory-token")
    );
    assert_eq!(core.secrets.status(GITHUB_TOKEN), SecretStatus::Present);
    assert!(
        !dir.path().join("grove.db").exists(),
        "the in-memory core never creates the durable database"
    );
}

#[test]
fn no_secret_value_reaches_the_database_or_the_logs() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    let sentinel = "ghp_SENTINEL_must_never_persist_9f2c1b7d";
    let capture = LogCapture::default();

    let store = Arc::new(Store::open(&paths.database).unwrap());
    let settings = SettingsService::new(Arc::clone(&store));
    let secrets = MemorySecretStore::new();

    tracing::subscriber::with_default(capture.clone(), || {
        // Proves the capture is not vacuous: this line must be visible afterwards.
        tracing::info!(target: "grove::test", "secret audit started");

        secrets.set(GITHUB_TOKEN, sentinel).unwrap();
        assert_eq!(
            secrets.get(GITHUB_TOKEN).unwrap().as_deref(),
            Some(sentinel)
        );
        assert_eq!(secrets.status(GITHUB_TOKEN), SecretStatus::Present);

        // A realistic settings write while the secret is held: settings must not absorb it.
        settings
            .set(SettingsPatch {
                harness_paths: Some(BTreeMap::from([(
                    "claude".to_string(),
                    "C:/Tools/claude.exe".to_string(),
                )])),
                theme: Some(Theme::Dark),
                ..Default::default()
            })
            .unwrap();

        // Debug output is the classic leak; the store prints key names only.
        let debug = format!("{secrets:?}");
        assert!(!debug.contains(sentinel), "Debug must redact secret values");

        secrets.delete(GITHUB_TOKEN).unwrap();
        assert_eq!(secrets.status(GITHUB_TOKEN), SecretStatus::Absent);
    });

    // Close the connection so any WAL contents are checkpointed before the byte scan.
    drop(store);
    drop(settings);

    let logs = capture.contents();
    assert!(
        logs.contains("secret audit started"),
        "the log capture works: {logs:?}"
    );
    assert!(
        !logs.contains(sentinel),
        "the secret value reached the logs: {logs:?}"
    );

    let bytes = read_database_bytes(&paths.database);
    assert!(!bytes.is_empty(), "the database has content to scan");
    assert!(
        !contains(&bytes, sentinel.as_bytes()),
        "the secret value reached the SQLite file"
    );
    // The settings write the test made really is in there, so the scan covers real rows.
    assert!(
        contains(&bytes, b"harnessPaths"),
        "the settings table is part of the scanned bytes"
    );
}

fn read_database_bytes(database: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    for path in [
        database.to_path_buf(),
        database.with_extension("db-wal"),
        database.with_extension("db-shm"),
    ] {
        if let Ok(mut contents) = std::fs::read(&path) {
            bytes.append(&mut contents);
        }
    }
    if bytes.is_empty() {
        // A WAL-mode database may keep the pages beside the main file; scan every sibling if needed.
        if let Some(parent) = database.parent() {
            for entry in std::fs::read_dir(parent).unwrap().flatten() {
                if entry.path().is_file() {
                    bytes.extend(std::fs::read(entry.path()).unwrap_or_default());
                }
            }
        }
    }
    bytes
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// A minimal `tracing` subscriber that appends every event, with its fields, to a string.
#[derive(Clone, Default)]
struct LogCapture(Arc<Mutex<String>>);

impl LogCapture {
    fn contents(&self) -> String {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl Subscriber for LogCapture {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        struct Writer<'a>(&'a mut String);

        impl Visit for Writer<'_> {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                use std::fmt::Write;
                let _ = write!(self.0, " {}={value:?}", field.name());
            }
        }

        let mut line = format!("[{}]", event.metadata().target());
        event.record(&mut Writer(&mut line));
        let mut logs = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        logs.push_str(&line);
        logs.push('\n');
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}
