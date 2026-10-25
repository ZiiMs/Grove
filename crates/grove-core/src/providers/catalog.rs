//! The provider catalog — the one place that names every vendor Grove can talk to.
//!
//! Adding a provider is one entry here plus one module when its wire format is new. Nothing else in
//! Grove changes: the settings pane, the model picker and the doctor all read this list.

use super::{AuthMethod, Capabilities, ProviderSpec, Wire};

/// Base URLs. Kept as constants so a test can assert the client joins paths onto them correctly.
pub const ANTHROPIC_BASE: &str = "https://api.anthropic.com";
pub const OPENAI_BASE: &str = "https://api.openai.com/v1";
pub const GOOGLE_BASE: &str = "https://generativelanguage.googleapis.com";
pub const OPENROUTER_BASE: &str = "https://openrouter.ai/api/v1";

/// Every provider this build ships, in display order.
pub fn builtin() -> Vec<ProviderSpec> {
    vec![
        ProviderSpec {
            id: "anthropic".into(),
            name: "Anthropic".into(),
            short: "AN".into(),
            tint: "#c08a63".into(),
            wire: Wire::Anthropic,
            base_url: ANTHROPIC_BASE.into(),
            auth: AuthMethod::ApiKey,
            key_url: "https://console.anthropic.com/settings/keys".into(),
            env_var: "ANTHROPIC_API_KEY".into(),
            models: vec![
                "claude-sonnet-4-5".into(),
                "claude-opus-4-1".into(),
                "claude-haiku-4-5".into(),
            ],
            default_model: "claude-sonnet-4-5".into(),
            capabilities: Capabilities::agent(),
        },
        ProviderSpec {
            id: "openai".into(),
            name: "OpenAI".into(),
            short: "OA".into(),
            tint: "#6f9ad1".into(),
            wire: Wire::OpenAi,
            base_url: OPENAI_BASE.into(),
            auth: AuthMethod::ApiKey,
            key_url: "https://platform.openai.com/api-keys".into(),
            env_var: "OPENAI_API_KEY".into(),
            models: vec!["gpt-5".into(), "gpt-5-mini".into(), "o4-mini".into()],
            default_model: "gpt-5".into(),
            capabilities: Capabilities::agent(),
        },
        ProviderSpec {
            id: "google".into(),
            name: "Google".into(),
            short: "GO".into(),
            tint: "#6fbf9a".into(),
            wire: Wire::Google,
            base_url: GOOGLE_BASE.into(),
            auth: AuthMethod::ApiKey,
            key_url: "https://aistudio.google.com/apikey".into(),
            env_var: "GEMINI_API_KEY".into(),
            models: vec!["gemini-2.5-pro".into(), "gemini-2.5-flash".into()],
            default_model: "gemini-2.5-pro".into(),
            capabilities: Capabilities::agent(),
        },
        ProviderSpec {
            id: "openrouter".into(),
            name: "OpenRouter".into(),
            short: "OR".into(),
            tint: "#9d86c9".into(),
            wire: Wire::OpenAi,
            base_url: OPENROUTER_BASE.into(),
            auth: AuthMethod::ApiKey,
            key_url: "https://openrouter.ai/keys".into(),
            env_var: "OPENROUTER_API_KEY".into(),
            // One key, many vendors: these are the ids OpenRouter itself routes by.
            models: vec![
                "anthropic/claude-sonnet-4.5".into(),
                "openai/gpt-5".into(),
                "google/gemini-2.5-pro".into(),
            ],
            default_model: "anthropic/claude-sonnet-4.5".into(),
            capabilities: Capabilities::agent(),
        },
    ]
}

/// One provider by id, or `None` for a provider this build does not register.
pub fn find(id: &str) -> Option<ProviderSpec> {
    builtin().into_iter().find(|spec| spec.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_providers_cover_three_wire_formats() {
        let specs = builtin();
        let wires: Vec<Wire> = specs.iter().map(|spec| spec.wire).collect();
        assert!(wires.contains(&Wire::Anthropic));
        assert!(wires.contains(&Wire::OpenAi));
        assert!(wires.contains(&Wire::Google));
        assert_eq!(
            wires.iter().filter(|wire| **wire == Wire::OpenAi).count(),
            2,
            "OpenAI and OpenRouter share one client"
        );
    }

    #[test]
    fn every_provider_uses_an_api_key_because_no_vendor_licenses_consumer_oauth() {
        // The decision, pinned: Anthropic, OpenAI and Google restrict subscription OAuth to their own
        // first-party clients. Offering it would put a user's account at risk.
        for spec in builtin() {
            assert_eq!(
                spec.auth,
                AuthMethod::ApiKey,
                "{} must use an API key",
                spec.id
            );
        }
    }

    #[test]
    fn openrouter_ships_vendor_qualified_model_ids() {
        let spec = find("openrouter").unwrap();
        assert!(
            spec.models.iter().all(|model| model.contains('/')),
            "OpenRouter routes by vendor/model"
        );
    }

    #[test]
    fn an_unknown_provider_is_none_not_a_panic() {
        assert!(find("not-a-vendor").is_none());
    }
}
