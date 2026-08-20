//! Claude rate limits.
//!
//! The figures come from the provider's own usage endpoint, using the OAuth
//! token Claude Code already holds. Nothing is proxied: the request goes from
//! this machine straight to Anthropic, which is the same arrangement Edith
//! describes, and no usage data is sent anywhere.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::credentials::{Credentials, RefreshResponse};
use crate::limits::LimitWindow;

/// Where the limits come from.
pub const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// Where an expired access token is exchanged for a fresh one.
pub const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
/// Claude Code's public OAuth client identifier.
pub const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
/// The beta header the usage endpoint requires.
pub const OAUTH_BETA: &str = "oauth-2025-04-20";

const REQUEST_TIMEOUT_SECS: u64 = 20;

/// A window that applies to particular models rather than the whole account.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopedWindow {
    /// What to call it, e.g. "Opus".
    pub label: String,
    pub window: LimitWindow,
}

/// Extra usage bought beyond the plan's included allowance.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraUsage {
    pub enabled: bool,
    pub used_credits: Option<f64>,
    pub monthly_limit: Option<f64>,
    pub currency: Option<String>,
}

/// Everything the usage endpoint reported.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeLimits {
    /// The five-hour rolling window.
    pub session: Option<LimitWindow>,
    /// The seven-day window.
    pub week: Option<LimitWindow>,
    /// Model-scoped weekly windows, when the account has any.
    pub scoped: Vec<ScopedWindow>,
    pub extra_usage: Option<ExtraUsage>,
}

impl ClaudeLimits {
    pub fn is_empty(&self) -> bool {
        self.session.is_none() && self.week.is_none() && self.scoped.is_empty()
    }
}

/// Read one window object.
fn window(value: &Value) -> Option<LimitWindow> {
    let object = value.as_object()?;
    // Older responses call it `percent`, current ones `utilization`.
    let percent = object
        .get("utilization")
        .or_else(|| object.get("percent"))
        .and_then(Value::as_f64)?;
    Some(LimitWindow {
        percent,
        resets_at: object.get("resets_at").and_then(parse_time),
    })
}

fn parse_time(value: &Value) -> Option<DateTime<Utc>> {
    let text = value.as_str()?;
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

/// Turn a response key like `seven_day_opus` into a label like "Opus".
pub fn scoped_label(key: &str) -> String {
    let suffix = key.trim_start_matches("seven_day_");
    suffix
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + chars.as_str()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse the usage endpoint's answer.
///
/// Two response shapes exist and both are handled: an older one carrying a
/// `limits` array of scoped windows, and the current one where each scoped
/// window is its own `seven_day_*` key. Keys whose value is null are simply
/// windows this account does not have.
pub fn parse_limits(body: &Value) -> ClaudeLimits {
    let mut limits = ClaudeLimits {
        session: body.get("five_hour").and_then(window),
        week: body.get("seven_day").and_then(window),
        ..Default::default()
    };

    // Current shape: seven_day_<something> keys alongside the main windows.
    if let Some(object) = body.as_object() {
        for (key, value) in object {
            if key == "seven_day" || !key.starts_with("seven_day_") {
                continue;
            }
            if let Some(parsed) = window(value) {
                limits.scoped.push(ScopedWindow {
                    label: scoped_label(key),
                    window: parsed,
                });
            }
        }
    }

    // Older shape: an array of scoped limits.
    if let Some(entries) = body.get("limits").and_then(Value::as_array) {
        for entry in entries {
            if entry.get("kind").and_then(Value::as_str) != Some("weekly_scoped") {
                continue;
            }
            let Some(parsed) = window(entry) else { continue };
            let label = entry
                .get("scope")
                .and_then(|scope| scope.get("model"))
                .and_then(|model| model.get("display_name"))
                .and_then(Value::as_str)
                .unwrap_or("Scoped")
                .to_string();
            limits.scoped.push(ScopedWindow {
                label,
                window: parsed,
            });
        }
    }

    // Stable order regardless of the map's iteration order.
    limits.scoped.sort_by(|a, b| a.label.cmp(&b.label));

    limits.extra_usage = body.get("extra_usage").and_then(|value| {
        let object = value.as_object()?;
        Some(ExtraUsage {
            enabled: object
                .get("is_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            used_credits: object.get("used_credits").and_then(Value::as_f64),
            monthly_limit: object.get("monthly_limit").and_then(Value::as_f64),
            currency: object
                .get("currency")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    });

    limits
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .user_agent(concat!("Veronica/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("cannot build an HTTPS client")
}

/// Exchange a refresh token for a fresh access token.
pub async fn refresh_token(refresh_token: &str) -> Result<RefreshResponse> {
    let response = client()?
        .post(TOKEN_URL)
        .json(&serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": CLIENT_ID,
        }))
        .send()
        .await
        .context("cannot reach the token endpoint")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED
        || status == reqwest::StatusCode::FORBIDDEN
        || status == reqwest::StatusCode::BAD_REQUEST
    {
        bail!("the saved Claude credentials were rejected; sign in with Claude Code again");
    }
    if !status.is_success() {
        bail!("the token endpoint answered HTTP {}", status.as_u16());
    }

    response
        .json::<RefreshResponse>()
        .await
        .context("the token endpoint returned an unexpected document")
}

/// Fetch the limits for an access token.
pub async fn fetch_limits(access_token: &str) -> Result<ClaudeLimits> {
    let response = client()?
        .get(USAGE_URL)
        // The token travels as a header, never as a process argument: /proc
        // makes argument lists readable to other processes.
        .bearer_auth(access_token)
        .header("anthropic-beta", OAUTH_BETA)
        .send()
        .await
        .context("cannot reach the usage endpoint")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        bail!("the Claude access token was rejected");
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        bail!("the usage endpoint is rate limiting; try again shortly");
    }
    if !status.is_success() {
        bail!("the usage endpoint answered HTTP {}", status.as_u16());
    }

    let body: Value = response
        .json()
        .await
        .context("the usage endpoint returned an unexpected document")?;
    Ok(parse_limits(&body))
}

/// Read the credentials, refreshing and saving if the token has expired, then
/// fetch the limits.
///
/// Returns `None` when there are no credentials at all, which means Claude Code
/// is not signed in on this machine — a normal state, not a failure.
pub async fn limits_for_user(now: DateTime<Utc>) -> Result<Option<ClaudeLimits>> {
    let Some(path) = Credentials::default_path() else {
        return Ok(None);
    };
    let Some(credentials) = Credentials::read(&path)? else {
        return Ok(None);
    };

    let mut access_token = credentials.access_token.clone();

    if credentials.needs_refresh(now) {
        match credentials.usable_refresh_token(now) {
            Some(refresh) => {
                let response = refresh_token(refresh).await?;
                access_token = response.access_token.clone();
                // Persisted because the provider may rotate the refresh token:
                // dropping a rotated one would invalidate the user's login.
                crate::credentials::ensure_owner_only(&path)?;
                Credentials::persist(&path, &credentials.applied(&response, now))?;
                tracing::debug!("refreshed the Claude access token");
            }
            None => bail!(
                "the saved Claude credentials have expired; sign in with Claude Code again"
            ),
        }
    }

    Ok(Some(fetch_limits(&access_token).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live response from this machine, values as returned.
    const CURRENT_SHAPE: &str = r#"{
      "five_hour": {
        "utilization": 81.0,
        "resets_at": "2026-08-20T18:59:59.946308+00:00",
        "limit_dollars": null, "used_dollars": null, "remaining_dollars": null
      },
      "seven_day": {
        "utilization": 45.0,
        "resets_at": "2026-08-20T16:59:59.946331+00:00",
        "limit_dollars": null
      },
      "seven_day_oauth_apps": null,
      "seven_day_opus": null,
      "seven_day_sonnet": null,
      "nimbus_quill": {"utilization": 0.0, "resets_at": null},
      "extra_usage": {
        "is_enabled": true, "monthly_limit": null,
        "used_credits": 0.0, "utilization": null, "currency": "USD"
      }
    }"#;

    /// The shape carrying a scoped-limits array.
    const ARRAY_SHAPE: &str = r#"{
      "five_hour": {"utilization": 10.0, "resets_at": "2026-08-20T18:00:00Z"},
      "seven_day": {"utilization": 20.0, "resets_at": "2026-08-25T18:00:00Z"},
      "limits": [
        {"kind": "weekly_scoped", "percent": 33.0,
         "resets_at": "2026-08-26T18:00:00Z",
         "scope": {"model": {"display_name": "Fable"}}},
        {"kind": "weekly", "percent": 99.0, "resets_at": null}
      ]
    }"#;

    #[test]
    fn parses_the_live_response() {
        let limits = parse_limits(&serde_json::from_str(CURRENT_SHAPE).unwrap());
        assert_eq!(limits.session.unwrap().percent, 81.0);
        let week = limits.week.unwrap();
        assert_eq!(week.percent, 45.0);
        assert!(week.resets_at.is_some(), "the reset time should parse");
    }

    #[test]
    fn null_scoped_windows_are_absent_rather_than_zero() {
        // seven_day_opus is null: the account has no such window, which is very
        // different from having one that is at 0%.
        let limits = parse_limits(&serde_json::from_str(CURRENT_SHAPE).unwrap());
        assert!(
            limits.scoped.is_empty(),
            "got {:?}",
            limits.scoped.iter().map(|s| &s.label).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reads_scoped_windows_from_the_named_keys() {
        let body = serde_json::json!({
            "five_hour": {"utilization": 1.0},
            "seven_day_opus": {"utilization": 12.0, "resets_at": "2026-08-26T18:00:00Z"},
            "seven_day_sonnet": {"utilization": 3.0, "resets_at": null},
        });
        let limits = parse_limits(&body);
        let labels: Vec<&str> = limits.scoped.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["Opus", "Sonnet"], "sorted for a stable order");
        assert_eq!(limits.scoped[0].window.percent, 12.0);
    }

    #[test]
    fn reads_scoped_windows_from_the_array_shape_too() {
        let limits = parse_limits(&serde_json::from_str(ARRAY_SHAPE).unwrap());
        assert_eq!(limits.session.unwrap().percent, 10.0);
        assert_eq!(limits.scoped.len(), 1, "only the weekly_scoped entry");
        assert_eq!(limits.scoped[0].label, "Fable");
        assert_eq!(limits.scoped[0].window.percent, 33.0);
    }

    #[test]
    fn accepts_either_percent_field_name() {
        let utilization = serde_json::json!({"five_hour": {"utilization": 7.0}});
        let percent = serde_json::json!({"five_hour": {"percent": 7.0}});
        assert_eq!(parse_limits(&utilization).session.unwrap().percent, 7.0);
        assert_eq!(parse_limits(&percent).session.unwrap().percent, 7.0);
    }

    #[test]
    fn the_seven_day_key_is_not_mistaken_for_a_scoped_window() {
        let limits = parse_limits(&serde_json::from_str(CURRENT_SHAPE).unwrap());
        assert!(limits.week.is_some());
        assert!(!limits.scoped.iter().any(|s| s.label.is_empty()));
    }

    #[test]
    fn extra_usage_is_read_when_present() {
        let limits = parse_limits(&serde_json::from_str(CURRENT_SHAPE).unwrap());
        let extra = limits.extra_usage.expect("extra usage should parse");
        assert!(extra.enabled);
        assert_eq!(extra.used_credits, Some(0.0));
        assert_eq!(extra.currency.as_deref(), Some("USD"));
        assert_eq!(extra.monthly_limit, None);
    }

    #[test]
    fn an_empty_response_is_empty_rather_than_a_panic() {
        let limits = parse_limits(&serde_json::json!({}));
        assert!(limits.is_empty());
        assert!(parse_limits(&serde_json::json!(null)).is_empty());
    }

    #[test]
    fn a_window_without_a_percentage_is_not_a_window() {
        let body = serde_json::json!({"five_hour": {"resets_at": "2026-08-20T18:00:00Z"}});
        assert!(parse_limits(&body).session.is_none());
    }

    #[test]
    fn scoped_labels_read_as_words() {
        // Only ever called for `seven_day_*` keys; the caller filters the rest.
        assert_eq!(scoped_label("seven_day_opus"), "Opus");
        assert_eq!(scoped_label("seven_day_oauth_apps"), "Oauth Apps");
        assert_eq!(scoped_label("seven_day_omelette"), "Omelette");
    }
}
