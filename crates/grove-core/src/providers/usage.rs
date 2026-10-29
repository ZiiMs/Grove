//! Plan usage outside a session turn: the vendor's own quota windows.
//!
//! OpenCode Go is a metered subscription whose 5-hour, weekly and monthly dollar budgets are
//! published at a dedicated endpoint. The response reports **used** percent; the panel shows what is
//! left, so the inversion happens here once. No other provider in this build exposes a plan endpoint;
//! asking for one returns `None` rather than inventing numbers.

use super::credentials::Credentials;
use super::{ProviderDeps, ProviderError};
use crate::ipc::{PlanUsage, PlanUsageWindow};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

const OPENCODE_GO_USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";
const WINDOWS: [(&str, &str); 3] = [
    ("rolling", "5-hour"),
    ("weekly", "Weekly"),
    ("monthly", "Monthly"),
];

/// Fetch the plan usage for `provider`, or `None` when that provider has no plan endpoint.
pub async fn plan_usage(
    provider: &str,
    deps: &ProviderDeps,
) -> Result<Option<PlanUsage>, ProviderError> {
    if provider != "opencode-go" {
        return Ok(None);
    }
    let (key, _) = Credentials::new(Arc::clone(&deps.secrets)).key(provider)?;
    let response = deps
        .http
        .get(OPENCODE_GO_USAGE_URL)
        .header("authorization", format!("Bearer {key}"))
        .send()
        .await
        .map_err(|error| ProviderError::Other(format!("usage request failed: {error}")))?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        ProviderError::Other(format!("usage response could not be read: {error}"))
    })?;
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(ProviderError::Unavailable(
            "OpenCode Go rejected the API key".into(),
        ));
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        return Err(ProviderError::Unavailable(
            "this key has no OpenCode Go subscription".into(),
        ));
    }
    if !status.is_success() {
        return Err(ProviderError::Api(format!(
            "usage request failed ({status}): {}",
            body.trim()
        )));
    }
    parse(provider, &body).map(Some)
}

fn parse(provider: &str, body: &str) -> Result<PlanUsage, ProviderError> {
    #[derive(Deserialize)]
    struct Envelope {
        usage: HashMap<String, Window>,
    }
    #[derive(Deserialize)]
    struct Window {
        #[serde(default = "ok_status")]
        status: String,
        percent: u8,
        #[serde(rename = "resetsAt", default)]
        resets_at: String,
    }
    fn ok_status() -> String {
        "ok".into()
    }

    let envelope: Envelope = serde_json::from_str(body)
        .map_err(|error| ProviderError::Protocol(format!("usage response: {error}")))?;
    let windows: Vec<PlanUsageWindow> = WINDOWS
        .iter()
        .filter_map(|(id, label)| {
            envelope.usage.get(*id).map(|window| {
                let used = window.percent.min(100);
                PlanUsageWindow {
                    id: (*id).into(),
                    label: (*label).into(),
                    used_percent: used,
                    remaining_percent: 100 - used,
                    status: window.status.clone(),
                    resets_at: window.resets_at.clone(),
                }
            })
        })
        .collect();
    if windows.is_empty() {
        return Err(ProviderError::Protocol(
            "usage response has no known windows".into(),
        ));
    }
    Ok(PlanUsage {
        provider: provider.into(),
        windows,
    })
}

#[cfg(test)]
mod tests {
    use super::parse;

    const SAMPLE: &str = r#"{
        "usage": {
            "rolling": { "status": "ok", "percent": 6, "resetsAt": "2026-10-29T15:50:00.000Z" },
            "weekly": { "status": "ok", "percent": 29, "resetsAt": "2026-11-01T19:00:00.000Z" },
            "monthly": { "status": "rate-limited", "percent": 74, "resetsAt": "2026-11-17T10:18:00.000Z" }
        }
    }"#;

    #[test]
    fn inverts_used_percent_into_what_is_left() {
        let usage = parse("opencode-go", SAMPLE).expect("the sample parses");
        assert_eq!(usage.windows.len(), 3);
        assert_eq!(usage.windows[0].label, "5-hour");
        assert_eq!(usage.windows[0].used_percent, 6);
        assert_eq!(usage.windows[0].remaining_percent, 94);
        assert_eq!(usage.windows[2].status, "rate-limited");
        assert_eq!(usage.windows[2].remaining_percent, 26);
        assert_eq!(usage.windows[1].resets_at, "2026-11-01T19:00:00.000Z");
    }

    #[test]
    fn clamps_an_out_of_range_percent_and_rejects_an_empty_response() {
        let usage = parse(
            "opencode-go",
            r#"{"usage":{"rolling":{"percent":140,"resetsAt":"2026-10-29T15:50:00Z"}}}"#,
        )
        .expect("one known window is enough");
        assert_eq!(usage.windows[0].used_percent, 100);
        assert_eq!(usage.windows[0].remaining_percent, 0);
        assert_eq!(usage.windows[0].status, "ok");

        let empty =
            parse("opencode-go", r#"{"usage":{}}"#).expect_err("no windows is a protocol error");
        assert!(empty.to_string().contains("no known windows"));
    }
}
