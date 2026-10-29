//! Golden-fixture half of the settings/doctor contract. The TS side parses the same files with zod
//! (`packages/contract/test/settings.test.ts`), so either side drifting breaks a test.

use grove_core::secrets::{SecretStatus, GITHUB_TOKEN};
use grove_core::settings::{SecretStatusView, Settings, Theme, TitleModel};
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
        catalog_sources: vec![
            "anthropics/skills".to_string(),
            "mattpocock/skills".to_string(),
        ],
        theme: Theme::Dark,
        auto_titles: false,
        title_model: Some(TitleModel {
            provider: "anthropic".into(),
            model: "claude-haiku-4-5".into(),
        }),
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
fn provider_statuses_match_fixture() {
    use grove_core::ipc::ProviderStatus;
    use grove_core::providers::credentials::KeySource;
    use grove_core::providers::{catalog, AuthMethod};

    let anthropic = catalog::find("anthropic").unwrap();
    let openai = catalog::find("openai").unwrap();
    let sample = vec![
        ProviderStatus {
            id: anthropic.id.clone(),
            name: anthropic.name.clone(),
            short: anthropic.short.clone(),
            tint: anthropic.tint.clone(),
            method: AuthMethod::ApiKey,
            key_url: anthropic.key_url.clone(),
            env_var: anthropic.env_var.clone(),
            default_model: anthropic.default_model.clone(),
            models: anthropic.models.clone(),
            configured: true,
            source: Some(KeySource::Keychain),
            detail: None,
        },
        ProviderStatus {
            id: openai.id.clone(),
            name: openai.name.clone(),
            short: openai.short.clone(),
            tint: openai.tint.clone(),
            method: AuthMethod::ApiKey,
            key_url: openai.key_url.clone(),
            env_var: openai.env_var.clone(),
            default_model: openai.default_model.clone(),
            models: openai.models.clone(),
            configured: false,
            source: None,
            detail: Some(format!(
                "Add an API key from {}, or set {}.",
                openai.key_url, openai.env_var
            )),
        },
    ];
    let value = serde_json::to_value(&sample).unwrap();
    assert_eq!(value, fixture("provider-statuses.json"));
    // The row exists to drive a settings pane; a key value must never appear in it.
    assert!(value[0].get("key").is_none());
}
