//! Golden-fixture half of the settings/doctor contract. The TS side parses the same files with zod
//! (`packages/contract/test/settings.test.ts`), so either side drifting breaks a test.

use grove_core::harness_doctor::{DoctorReport, HarnessHealth, ToolHealth};
use grove_core::secrets::{SecretStatus, GITHUB_TOKEN};
use grove_core::settings::{SecretStatusView, Settings, Theme};
use std::collections::BTreeMap;

fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract/fixtures")
        .join(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read fixture {}: {err}", path.display()));
    serde_json::from_str(&raw).expect("fixture is valid JSON")
}

#[test]
fn settings_matches_fixture() {
    let sample = Settings {
        harness_paths: BTreeMap::from([
            ("claude".to_string(), "C:/Tools/claude.exe".to_string()),
            ("mock".to_string(), "C:/Tools/fake-harness.cmd".to_string()),
        ]),
        catalog_sources: vec![
            "anthropics/skills".to_string(),
            "mattpocock/skills".to_string(),
        ],
        theme: Theme::Dark,
        extra: BTreeMap::from([("futureFlag".to_string(), serde_json::json!(true))]),
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("settings.json")
    );
}

#[test]
fn secret_status_matches_fixture_and_never_carries_a_value() {
    let sample = SecretStatusView {
        key: GITHUB_TOKEN.to_string(),
        status: SecretStatus::Present,
    };
    let value = serde_json::to_value(&sample).unwrap();
    assert_eq!(value, fixture("secret-status.json"));
    assert_eq!(value.as_object().unwrap().len(), 2, "key and status only");
}

#[test]
fn doctor_report_matches_fixture() {
    let sample = DoctorReport {
        harnesses: vec![
            HarnessHealth {
                id: "mock".to_string(),
                label: "Mock (builtin)".to_string(),
                installed: true,
                version: Some("0.1.0".to_string()),
                path: Some("builtin:mock".to_string()),
                detail: None,
            },
            HarnessHealth {
                id: "claude".to_string(),
                label: "Claude Code".to_string(),
                installed: false,
                version: None,
                path: Some("C:/Tools/claude.exe".to_string()),
                detail: Some("configured path does not exist: C:/Tools/claude.exe".to_string()),
            },
        ],
        tools: vec![
            ToolHealth {
                id: "node".to_string(),
                label: "Node.js".to_string(),
                installed: true,
                version: Some("v20.11.1".to_string()),
                path: Some("C:/Program Files/nodejs/node.exe".to_string()),
                detail: None,
            },
            ToolHealth {
                id: "git".to_string(),
                label: "Git".to_string(),
                installed: true,
                version: Some("git version 2.43.0.windows.1".to_string()),
                path: Some("C:/Program Files/Git/cmd/git.exe".to_string()),
                detail: None,
            },
            ToolHealth {
                id: "gh".to_string(),
                label: "GitHub CLI".to_string(),
                installed: false,
                version: None,
                path: None,
                detail: Some("gh is not installed or not on PATH".to_string()),
            },
        ],
        checked_at: "2026-10-24T18:00:00.000Z".to_string(),
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("doctor.json")
    );
}
