//! Codex rate limits.
//!
//! Codex exposes its own limits over a JSON-RPC conversation on stdio rather
//! than an HTTP endpoint, so there is no network request and no token for
//! Veronica to handle: `codex app-server` is asked, and it uses whatever
//! credentials the user already signed in with.
//!
//! The protocol is: initialize, initialized, then `account/rateLimits/read`.
//! The reply carries a primary and a secondary window, distinguished by their
//! duration rather than by name, so the shorter one is the session window and
//! the longer one the weekly window.

use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

/// How long to wait for the whole exchange before giving up.
pub const READ_TIMEOUT: Duration = Duration::from_secs(20);

use crate::limits::LimitWindow;

/// One window as Codex reports it.
#[derive(Debug, Clone, Deserialize)]
struct RawWindow {
    #[serde(rename = "usedPercent")]
    used_percent: Option<f64>,
    #[serde(rename = "resetsAt")]
    resets_at: Option<f64>,
    #[serde(rename = "windowDurationMins")]
    window_duration_mins: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CodexLimits {
    pub session: Option<LimitWindow>,
    pub week: Option<LimitWindow>,
}

impl CodexLimits {
    pub fn is_empty(&self) -> bool {
        self.session.is_none() && self.week.is_none()
    }
}

fn to_window(raw: &RawWindow) -> Option<(f64, LimitWindow)> {
    let percent = raw.used_percent?;
    let resets_at = raw.resets_at.and_then(|seconds| {
        chrono::DateTime::from_timestamp(seconds as i64, 0)
    });
    Some((
        raw.window_duration_mins.unwrap_or(0.0),
        LimitWindow { percent, resets_at },
    ))
}

/// Map the reply's rate-limit snapshot onto session and weekly windows.
///
/// Ordered by duration rather than trusting the `primary`/`secondary` naming,
/// which says nothing about which window is which.
pub fn parse_snapshot(snapshot: &Value) -> CodexLimits {
    let mut windows: Vec<(f64, LimitWindow)> = ["primary", "secondary"]
        .iter()
        .filter_map(|key| snapshot.get(*key))
        .filter_map(|value| serde_json::from_value::<RawWindow>(value.clone()).ok())
        .filter_map(|raw| to_window(&raw))
        .collect();

    windows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut limits = CodexLimits::default();
    let mut iter = windows.into_iter();
    if let Some((_, window)) = iter.next() {
        limits.session = Some(window);
    }
    if let Some((_, window)) = iter.next() {
        limits.week = Some(window);
    }
    limits
}

/// Whether the codex command is available at all.
pub fn is_available() -> bool {
    veronica_core::session::which("codex").is_some()
}

/// Ask `codex app-server` for the account's rate limits.
pub async fn fetch_limits() -> Result<CodexLimits> {
    if !is_available() {
        bail!("the codex command is not installed");
    }

    let mut child = Command::new("codex")
        .arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("cannot start codex app-server")?;

    let mut stdin = child.stdin.take().context("no stdin for codex")?;
    let stdout = child.stdout.take().context("no stdout for codex")?;

    let exchange = async {
        let mut lines = BufReader::new(stdout).lines();

        let mut send = |value: Value| {
            let mut line = serde_json::to_string(&value).unwrap_or_default();
            line.push('\n');
            line
        };

        stdin
            .write_all(
                send(json!({
                    "method": "initialize",
                    "id": 0,
                    "params": {
                        "clientInfo": {
                            "name": "veronica",
                            "title": "Veronica",
                            "version": env!("CARGO_PKG_VERSION"),
                        }
                    }
                }))
                .as_bytes(),
            )
            .await?;
        stdin.flush().await?;

        // Wait for the initialize reply before going on, as the protocol
        // requires; sending the rest first can be rejected.
        await_response(&mut lines, 0).await?;

        stdin
            .write_all(send(json!({"method": "initialized", "params": {}})).as_bytes())
            .await?;
        stdin
            .write_all(
                send(json!({"method": "account/rateLimits/read", "id": 1, "params": {}}))
                    .as_bytes(),
            )
            .await?;
        stdin.flush().await?;

        let reply = await_response(&mut lines, 1).await?;
        let snapshot = reply
            .get("result")
            .and_then(|result| result.get("rateLimits"))
            .cloned()
            .context("codex reported no rate limits")?;
        Ok::<CodexLimits, anyhow::Error>(parse_snapshot(&snapshot))
    };

    let result = tokio::time::timeout(READ_TIMEOUT, exchange)
        .await
        .context("codex app-server stopped responding")?;

    // The child is killed on drop, so a hung server cannot outlive this call.
    let _ = child.start_kill();
    result
}

/// Read lines until the reply with this id arrives.
async fn await_response<R>(
    lines: &mut tokio::io::Lines<BufReader<R>>,
    id: i64,
) -> Result<Value>
where
    R: tokio::io::AsyncRead + Unpin,
{
    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            // The server may log non-JSON to stdout; skip rather than fail.
            continue;
        };
        if value.get("id").and_then(Value::as_i64) == Some(id) {
            if let Some(error) = value.get("error") {
                bail!("codex refused the request: {error}");
            }
            return Ok(value);
        }
    }
    bail!("codex closed the connection before answering")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorter_window_is_the_session_regardless_of_naming() {
        // `primary` is the weekly window here; ordering by duration is what
        // keeps the two straight.
        let snapshot = json!({
            "primary":   {"usedPercent": 45.0, "resetsAt": 1787200000, "windowDurationMins": 10080},
            "secondary": {"usedPercent": 81.0, "resetsAt": 1787100000, "windowDurationMins": 300}
        });
        let limits = parse_snapshot(&snapshot);
        assert_eq!(limits.session.unwrap().percent, 81.0);
        assert_eq!(limits.week.unwrap().percent, 45.0);
    }

    #[test]
    fn a_single_window_becomes_the_session() {
        let snapshot = json!({
            "primary": {"usedPercent": 12.0, "windowDurationMins": 300}
        });
        let limits = parse_snapshot(&snapshot);
        assert_eq!(limits.session.unwrap().percent, 12.0);
        assert!(limits.week.is_none());
    }

    #[test]
    fn reset_times_are_read_from_unix_seconds() {
        let snapshot = json!({
            "primary": {"usedPercent": 5.0, "resetsAt": 1787100000, "windowDurationMins": 300}
        });
        let window = parse_snapshot(&snapshot).session.unwrap();
        assert_eq!(window.resets_at.unwrap().timestamp(), 1787100000);
    }

    #[test]
    fn a_window_without_a_percentage_is_skipped() {
        let snapshot = json!({
            "primary": {"resetsAt": 1787100000, "windowDurationMins": 300},
            "secondary": {"usedPercent": 20.0, "windowDurationMins": 10080}
        });
        let limits = parse_snapshot(&snapshot);
        // Only the usable window survives, and it lands in the session slot.
        assert_eq!(limits.session.unwrap().percent, 20.0);
        assert!(limits.week.is_none());
    }

    #[test]
    fn an_empty_snapshot_is_empty() {
        assert!(parse_snapshot(&json!({})).is_empty());
        assert!(parse_snapshot(&json!(null)).is_empty());
    }

    #[tokio::test]
    async fn a_missing_codex_command_reports_clearly() {
        // This machine has no codex binary; the message should say so rather
        // than surfacing a spawn error.
        if is_available() {
            return;
        }
        let error = fetch_limits().await.unwrap_err().to_string();
        assert!(error.contains("not installed"), "got: {error}");
    }
}
