use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

pub const READ_TIMEOUT: Duration = Duration::from_secs(20);

use crate::limits::LimitWindow;

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
    if !percent.is_finite() || !(0.0..=100.0).contains(&percent) {
        return None;
    }
    let resets_at = raw
        .resets_at
        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds as i64, 0));
    Some((
        raw.window_duration_mins.unwrap_or(0.0),
        LimitWindow { percent, resets_at },
    ))
}

pub fn parse_snapshot(snapshot: &Value) -> CodexLimits {
    let mut limits = CodexLimits::default();
    for key in ["primary", "secondary"] {
        let Some((duration, window)) = snapshot
            .get(key)
            .and_then(|value| serde_json::from_value::<RawWindow>(value.clone()).ok())
            .and_then(|raw| to_window(&raw))
        else {
            continue;
        };
        let weekly = if duration > 0.0 {
            duration > 24.0 * 60.0
        } else {
            key == "secondary"
        };
        if weekly {
            limits.week = Some(window);
        } else {
            limits.session = Some(window);
        }
    }
    limits
}

pub fn is_available() -> bool {
    codex_binary().is_some()
}

pub fn codex_binary() -> Option<PathBuf> {
    let override_path = std::env::var_os("VERONICA_CODEX_BIN").map(PathBuf::from);
    codex_binary_from(
        override_path,
        veronica_core::session::which("codex"),
        veronica_core::paths::home_dir().as_deref(),
    )
}

fn codex_binary_from(
    override_path: Option<PathBuf>,
    path_match: Option<PathBuf>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    candidates.extend(override_path);
    candidates.extend(path_match);
    candidates.push(PathBuf::from("/usr/lib/chatgpt/resources/codex"));
    candidates.push(PathBuf::from("/usr/local/bin/codex"));
    candidates.push(PathBuf::from("/usr/bin/codex"));
    if let Some(home) = home {
        candidates.push(home.join(".local/bin/codex"));
        candidates.push(home.join(".npm-global/bin/codex"));
    }
    candidates
        .into_iter()
        .find(|path| veronica_core::host::is_executable(path))
}

pub async fn fetch_limits() -> Result<CodexLimits> {
    if !is_available() {
        bail!("the codex command is not installed");
    }

    let binary = codex_binary().context("the codex command is not installed")?;
    let mut child = Command::from(veronica_core::host::command(&binary))
        .arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("cannot start {} app-server", binary.display()))?;

    let mut stdin = child.stdin.take().context("no stdin for codex")?;
    let stdout = child.stdout.take().context("no stdout for codex")?;
    let stderr = child.stderr.take().context("no stderr for codex")?;
    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut messages = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            if !line.trim().is_empty() {
                messages.push(line);
            }
        }
        messages
    });

    let exchange = async {
        let mut lines = BufReader::new(stdout).lines();

        let send = |value: Value| {
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

        await_response(&mut lines, 0).await?;

        stdin
            .write_all(send(json!({"method": "initialized"})).as_bytes())
            .await?;
        stdin
            .write_all(
                send(json!({"method": "account/rateLimits/read", "id": 1, "params": null}))
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

    let _ = child.start_kill();
    let diagnostics = stderr_task.await.unwrap_or_default();
    result.with_context(|| {
        diagnostics.last().map_or_else(
            || "codex app-server returned no diagnostics".to_string(),
            |line| format!("codex app-server: {line}"),
        )
    })
}

async fn await_response<R>(lines: &mut tokio::io::Lines<BufReader<R>>, id: i64) -> Result<Value>
where
    R: tokio::io::AsyncRead + Unpin,
{
    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
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
        assert!(limits.session.is_none());
        assert_eq!(limits.week.unwrap().percent, 20.0);
    }

    #[test]
    fn a_lone_weekly_primary_stays_weekly() {
        let limits = parse_snapshot(&json!({
            "primary": {"usedPercent": 32.0, "windowDurationMins": 10080}
        }));
        assert!(limits.session.is_none());
        assert_eq!(limits.week.unwrap().percent, 32.0);
    }

    #[test]
    fn unknown_secondary_duration_does_not_become_session_usage() {
        let limits = parse_snapshot(&json!({"secondary": {"usedPercent": 55.0}}));
        assert!(limits.session.is_none());
        assert_eq!(limits.week.unwrap().percent, 55.0);
    }

    #[test]
    fn invalid_percentages_are_unavailable_instead_of_fabricated() {
        for percent in [-1.0, 101.0] {
            assert!(parse_snapshot(&json!({"primary": {"usedPercent": percent}})).is_empty());
        }
    }

    #[test]
    fn an_empty_snapshot_is_empty() {
        assert!(parse_snapshot(&json!({})).is_empty());
        assert!(parse_snapshot(&json!(null)).is_empty());
    }

    #[test]
    fn desktop_install_is_found_without_a_shell_path() {
        let binary = PathBuf::from("/usr/lib/chatgpt/resources/codex");
        if binary.exists() {
            assert_eq!(codex_binary_from(None, None, None), Some(binary));
        }
    }

    #[tokio::test]
    async fn a_missing_codex_command_reports_clearly() {
        if is_available() {
            return;
        }
        let error = fetch_limits().await.unwrap_err().to_string();
        assert!(error.contains("not installed"), "got: {error}");
    }
}
