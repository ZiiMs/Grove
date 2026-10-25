//! Doctor behaviour against fake binaries in temp dirs: a version, a non-zero exit, a missing binary
//! and a hung process. Every assertion is about a report row; none of these situations may panic or
//! turn into an error that would break the Settings panel.

use grove_core::harness_doctor::{DoctorConfig, HarnessDoctor};
use grove_core::paths::{display_path, AppPaths};
use grove_core::sessions::manager::SessionManager;
use grove_core::settings::{SettingsPatch, SettingsService};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// A temp directory of fake `--version` programs. On Windows they are `.cmd` files (Rust's `Command`
/// runs batch files); elsewhere they are executable shell scripts.
struct FakeBins {
    dir: TempDir,
}

impl FakeBins {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("temp dir"),
        }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn path_env(&self) -> String {
        self.path().to_string_lossy().into_owned()
    }

    fn write(&self, name: &str, script: &str) -> PathBuf {
        #[cfg(windows)]
        {
            let path = self.path().join(format!("{name}.cmd"));
            std::fs::write(
                &path,
                format!("@echo off\r\n{}\r\n", script.replace('\n', "\r\n")),
            )
            .expect("write fake binary");
            path
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = self.path().join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).expect("write fake binary");
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&path, permissions).unwrap();
            path
        }
    }

    fn version(&self, name: &str, output: &str) -> PathBuf {
        self.write(name, &format!("echo {output}"))
    }

    fn failing(&self, name: &str, message: &str, code: i32) -> PathBuf {
        #[cfg(windows)]
        let script = format!("echo {message} 1>&2\nexit /b {code}");
        #[cfg(not(windows))]
        let script = format!("echo {message} >&2\nexit {code}");
        self.write(name, &script)
    }

    fn hanging(&self, name: &str) -> PathBuf {
        #[cfg(windows)]
        let script = "ping -n 10 127.0.0.1 >nul\necho never";
        #[cfg(not(windows))]
        let script = "sleep 10\necho never";
        self.write(name, script)
    }
}

fn doctor(
    dir: &Path,
    path_env: String,
    timeout: Duration,
) -> (HarnessDoctor, Arc<SettingsService>) {
    let paths = AppPaths::new(dir.join("app-data"));
    paths.ensure().unwrap();
    let manager = Arc::new(SessionManager::in_memory(&paths).unwrap());
    let settings = Arc::new(SettingsService::new(manager.store()));
    let doctor = HarnessDoctor::with_config(
        Arc::clone(&settings),
        Arc::clone(&manager),
        DoctorConfig {
            path_env: Some(path_env),
            path_extensions: None,
            timeout,
        },
    );
    (doctor, settings)
}

fn override_path(settings: &SettingsService, path: &Path) {
    settings
        .set(SettingsPatch {
            harness_paths: Some(BTreeMap::from([("mock".to_string(), display_path(path))])),
            ..Default::default()
        })
        .unwrap();
}

#[tokio::test]
async fn reports_builtin_harnesses_and_the_node_git_gh_tools() {
    let bins = FakeBins::new();
    bins.version("node", "fake-node v20.11.1");
    bins.version("git", "git version 2.43.0");
    bins.failing("gh", "gh: not authenticated", 4);
    let (doctor, _settings) = doctor(bins.path(), bins.path_env(), Duration::from_secs(5));

    let report = doctor.report().await;

    let harness = &report.harnesses[0];
    assert_eq!(
        report
            .harnesses
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        ["mock", "claude", "acp", "pi", "opencode", "codex"],
        "the doctor screens the same five harnesses (plus the mock) the picker offers"
    );
    assert!(
        report
            .harnesses
            .iter()
            .all(|row| !row.label.trim().is_empty()),
        "every harness row is labelled for the Settings table"
    );
    // The label is the adapter's own `display_name()`, read from the registry: a sixth harness needs
    // no new match arm here.
    for row in &report.harnesses {
        let expected = grove_core::adapters::registry::builtin_registry()
            .iter()
            .find(|adapter| adapter.id().as_str() == row.id)
            .map(|adapter| adapter.display_name())
            .unwrap_or(row.id.as_str());
        assert_eq!(row.label, expected, "harness {} label", row.id);
    }
    assert_eq!(harness.id, "mock");
    assert!(harness.installed, "the mock adapter is built in");
    assert_eq!(harness.path.as_deref(), Some("builtin:mock"));
    assert_eq!(harness.label, "Mock (builtin)");
    assert!(!report.checked_at.is_empty());

    let node = report.tools.iter().find(|tool| tool.id == "node").unwrap();
    assert!(node.installed);
    assert_eq!(node.version.as_deref(), Some("fake-node v20.11.1"));
    assert!(
        node.path.as_deref().unwrap().ends_with("node.cmd")
            || node.path.as_deref().unwrap().ends_with("/node")
    );

    let git = report.tools.iter().find(|tool| tool.id == "git").unwrap();
    assert!(git.installed);
    assert_eq!(git.version.as_deref(), Some("git version 2.43.0"));

    let gh = report.tools.iter().find(|tool| tool.id == "gh").unwrap();
    assert!(
        !gh.installed,
        "a non-zero exit is reported, not treated as installed"
    );
    assert!(gh.version.is_none());
    assert!(
        gh.detail
            .as_deref()
            .unwrap()
            .contains("exited with status 4"),
        "{:?}",
        gh.detail
    );
    assert!(
        gh.detail
            .as_deref()
            .unwrap()
            .contains("gh: not authenticated"),
        "{:?}",
        gh.detail
    );
}

#[tokio::test]
async fn a_configured_path_wins_and_wrong_or_failing_overrides_are_explained() {
    let bins = FakeBins::new();
    let fake = bins.version("mock-harness", "fake-harness 9.9.9");
    let (doctor, settings) = doctor(bins.path(), bins.path_env(), Duration::from_secs(5));

    override_path(&settings, &fake);
    let report = doctor.report().await;
    let harness = &report.harnesses[0];
    assert!(harness.installed);
    assert_eq!(harness.version.as_deref(), Some("fake-harness 9.9.9"));
    assert_eq!(harness.path.as_deref(), Some(display_path(&fake).as_str()));

    let missing = bins.path().join("does-not-exist.exe");
    override_path(&settings, &missing);
    let report = doctor.report().await;
    let harness = &report.harnesses[0];
    assert!(!harness.installed);
    assert_eq!(
        harness.path.as_deref(),
        Some(display_path(&missing).as_str())
    );
    assert!(
        harness
            .detail
            .as_deref()
            .unwrap()
            .contains("configured path does not exist"),
        "{:?}",
        harness.detail
    );

    let failing = bins.failing("broken-harness", "harness exploded", 7);
    override_path(&settings, &failing);
    let report = doctor.report().await;
    let harness = &report.harnesses[0];
    assert!(!harness.installed);
    assert_eq!(
        harness.path.as_deref(),
        Some(display_path(&failing).as_str())
    );
    assert!(
        harness
            .detail
            .as_deref()
            .unwrap()
            .contains("exited with status 7"),
        "{:?}",
        harness.detail
    );
    assert!(
        harness
            .detail
            .as_deref()
            .unwrap()
            .contains("harness exploded"),
        "{:?}",
        harness.detail
    );
}

#[tokio::test]
async fn a_hung_binary_times_out_instead_of_hanging_the_panel() {
    let bins = FakeBins::new();
    let slow = bins.hanging("slow-harness");
    let (doctor, settings) = doctor(bins.path(), bins.path_env(), Duration::from_millis(150));
    override_path(&settings, &slow);

    let started = Instant::now();
    let report = doctor.report().await;
    let harness = &report.harnesses[0];

    assert!(!harness.installed);
    assert!(
        harness
            .detail
            .as_deref()
            .unwrap()
            .contains("timed out after"),
        "{:?}",
        harness.detail
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the timeout actually cut the probe short"
    );
    eprintln!("test body ending at {:?}", started.elapsed());
    drop(bins);
    eprintln!("bins dropped at {:?}", started.elapsed());
}

#[tokio::test]
async fn an_empty_path_reports_missing_tools_without_failing_the_report() {
    let empty = tempfile::tempdir().unwrap();
    let (doctor, _settings) = doctor(
        empty.path(),
        empty.path().to_string_lossy().into_owned(),
        Duration::from_secs(5),
    );

    let report = doctor.report().await;

    assert_eq!(report.tools.len(), 3);
    for tool in &report.tools {
        assert!(!tool.installed, "{} is not installed", tool.id);
        assert!(
            tool.detail
                .as_deref()
                .unwrap()
                .contains("not installed or not on PATH"),
            "{:?}",
            tool.detail
        );
    }
    // The builtin harness is unaffected by an empty PATH.
    assert!(report.harnesses[0].installed);
}

#[tokio::test]
async fn path_lookup_takes_the_first_match() {
    let first = FakeBins::new();
    first.version("node", "first-node 1.0.0");
    let second = FakeBins::new();
    second.version("node", "second-node 2.0.0");
    let path_env = std::env::join_paths([first.path(), second.path()])
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let (doctor, _settings) = doctor(first.path(), path_env, Duration::from_secs(5));

    let report = doctor.report().await;
    let node = report.tools.iter().find(|tool| tool.id == "node").unwrap();
    assert_eq!(node.version.as_deref(), Some("first-node 1.0.0"));
}
