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
pub const OPENCODE_GO_BASE: &str = "https://opencode.ai/inference/go/openai/v1";
/// The Go gateway lists every subscription model here, outside the OpenAI-compatible prefix.
pub const OPENCODE_GO_MODELS_URL: &str = "https://opencode.ai/inference/go/v1/models";

/// Transport details a gateway needs that do not belong in the picker's [`ProviderSpec`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Transport {
    /// Where the key's model list lives when it is not under `base_url`.
    pub models_url: Option<&'static str>,
    /// A request header carrying the conversation id, for a gateway that routes by session.
    pub session_header: Option<&'static str>,
}

/// The transport extras for one provider. Unknown ids get the plain wire behaviour.
pub fn transport(id: &str) -> Transport {
    match id {
        "opencode-go" => Transport {
            models_url: Some(OPENCODE_GO_MODELS_URL),
            session_header: Some("x-opencode-session"),
        },
        _ => Transport::default(),
    }
}

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
        ProviderSpec {
            id: "opencode-go".into(),
            name: "OpenCode Go".into(),
            short: "OG".into(),
            tint: "#b08ce0".into(),
            wire: Wire::OpenAi,
            base_url: OPENCODE_GO_BASE.into(),
            auth: AuthMethod::ApiKey,
            key_url: "https://opencode.ai/console".into(),
            env_var: "OPENCODE_API_KEY".into(),
            // The Go subscription's chat-completions models. Models served on Go's `/responses`,
            // Anthropic or Google endpoints need a different client and are not in this build.
            models: vec![
                "kimi-k3".into(),
                "kimi-k3.1-pro".into(),
                "glm-5.3".into(),
                "glm-5.3-flash".into(),
                "glm-5.2".into(),
                "deepseek-v4.1-flash".into(),
                "deepseek-v4-pro".into(),
                "minimax-m3.1-flash".into(),
                "mimo-v2.6-pro".into(),
                "mimo-v2.6-flash".into(),
                "mimo-v2.5-pro".into(),
                "mimo-v2.5".into(),
                "kimi-k2.7-code".into(),
                "longcat-2.0".into(),
                "step-5-preview".into(),
                "hy4-preview".into(),
                "hy3".into(),
                "beam-free".into(),
            ],
            default_model: "kimi-k3".into(),
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
    fn the_shipped_providers_cover_three_wire_formats() {
        let specs = builtin();
        let wires: Vec<Wire> = specs.iter().map(|spec| spec.wire).collect();
        assert!(wires.contains(&Wire::Anthropic));
        assert!(wires.contains(&Wire::OpenAi));
        assert!(wires.contains(&Wire::Google));
        assert_eq!(
            wires.iter().filter(|wire| **wire == Wire::OpenAi).count(),
            3,
            "OpenAI, OpenRouter and OpenCode Go share one client"
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
    fn opencode_go_speaks_openai_and_declares_its_session_header() {
        let spec = find("opencode-go").expect("OpenCode Go ships");
        assert_eq!(spec.wire, Wire::OpenAi);
        assert_eq!(spec.base_url, "https://opencode.ai/inference/go/openai/v1");
        assert_eq!(spec.tint, "#b08ce0");
        let transport = transport("opencode-go");
        assert_eq!(transport.models_url, Some(OPENCODE_GO_MODELS_URL));
        assert_eq!(transport.session_header, Some("x-opencode-session"));
        assert_eq!(spec.env_var, "OPENCODE_API_KEY");
        assert_eq!(spec.key_url, "https://opencode.ai/console");
        assert!(spec.models.contains(&spec.default_model));
        assert!(spec.models.iter().any(|model| model == "glm-5.3"));
    }

    #[test]
    fn only_a_gateway_declares_transport_extras() {
        assert_eq!(transport("anthropic"), Transport::default());
        assert_eq!(transport("not-a-provider"), Transport::default());
    }

    #[test]
    fn an_unknown_provider_is_none_not_a_panic() {
        assert!(find("not-a-vendor").is_none());
    }
}
