use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::models::UsageDocument;

pub const COLLECTOR_SCRIPT: &str = include_str!("../../../resources/refresh-usage");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CollectorEvent {
    Phase {
        name: String,
        detail: String,
        seconds: f64,
    },
    Note {
        message: String,
    },
    Summary {
        name: String,
        detail: String,
    },
    Error {
        message: String,
    },
    Done {
        seconds: f64,
    },
    Unknown {
        line: String,
    },
}

impl CollectorEvent {
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() {
            return None;
        }
        let mut fields = line.split('\t');
        let kind = fields.next()?;
        let rest: Vec<&str> = fields.collect();

        let seconds = |raw: Option<&&str>| {
            raw.and_then(|s| s.trim().parse::<f64>().ok())
                .unwrap_or_default()
        };

        Some(match kind {
            "phase" => Self::Phase {
                name: rest.first().unwrap_or(&"").to_string(),
                detail: rest.get(1).unwrap_or(&"").to_string(),
                seconds: seconds(rest.get(2)),
            },
            "note" => Self::Note {
                message: rest.join("\t"),
            },
            "summary" => Self::Summary {
                name: rest.first().unwrap_or(&"").to_string(),
                detail: rest.get(1..).map(|r| r.join("\t")).unwrap_or_default(),
            },
            "error" => Self::Error {
                message: rest.join("\t"),
            },
            "done" => Self::Done {
                seconds: seconds(rest.first()),
            },
            _ => Self::Unknown {
                line: line.to_string(),
            },
        })
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Done { .. })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshOutcome {
    pub events: Vec<CollectorEvent>,
    pub document: UsageDocument,
    pub completed: bool,
}

impl RefreshOutcome {
    pub fn summaries(&self) -> Vec<(&str, &str)> {
        self.events
            .iter()
            .filter_map(|event| match event {
                CollectorEvent::Summary { name, detail } => Some((name.as_str(), detail.as_str())),
                _ => None,
            })
            .collect()
    }

    pub fn errors(&self) -> Vec<&str> {
        self.events
            .iter()
            .filter_map(|event| match event {
                CollectorEvent::Error { message } => Some(message.as_str()),
                _ => None,
            })
            .collect()
    }
}

pub fn install_script(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    std::fs::write(path, COLLECTOR_SCRIPT)
        .with_context(|| format!("cannot write {}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

pub fn read_document(path: &Path) -> Result<Option<UsageDocument>> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let doc: UsageDocument = serde_json::from_slice(&bytes)
                .with_context(|| format!("{} is not a usage document", path.display()))?;
            if !doc.is_compatible() {
                bail!(
                    "{} uses schema version {} but this build reads version {}",
                    path.display(),
                    doc.schema_version,
                    crate::models::SCHEMA_VERSION
                );
            }
            Ok(Some(doc))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("cannot read {}", path.display())),
    }
}

pub async fn refresh<F>(
    script: &Path,
    output_dir: &Path,
    cache_dir: &Path,
    mut on_event: F,
) -> Result<RefreshOutcome>
where
    F: FnMut(&CollectorEvent),
{
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("cannot create {}", output_dir.display()))?;

    let nice = veronica_core::host::which("nice");
    let program = nice.as_deref().unwrap_or_else(|| Path::new("bash"));
    let mut command = Command::from(veronica_core::host::command_with_env(
        program,
        &[("EDITH_CACHE_DIR", cache_dir.as_os_str())],
    ));
    if nice.is_some() {
        command.args(["-n", "10", "bash"]);
    }
    let mut child = command
        .arg(script)
        .arg(output_dir)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("cannot run {}", script.display()))?;

    let stdout = child.stdout.take().context("collector stdout missing")?;
    let stderr = child.stderr.take().context("collector stderr missing")?;

    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut collected = std::collections::VecDeque::new();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(target: "veronica::collector", "{line}");
            if collected.len() == 200 {
                collected.pop_front();
            }
            collected.push_back(line);
        }
        collected.into_iter().collect::<Vec<_>>()
    });

    let mut events = Vec::new();
    let mut completed = false;
    let mut lines = BufReader::new(stdout).lines();
    while let Some(line) = lines.next_line().await? {
        if let Some(event) = CollectorEvent::parse(&line) {
            completed |= event.is_terminal();
            on_event(&event);
            events.push(event);
        }
    }

    let status = child.wait().await?;
    let stderr_lines = stderr_task.await.unwrap_or_default();

    if !status.success() && !completed {
        let reported = events.iter().rev().find_map(|event| match event {
            CollectorEvent::Error { message } if !message.trim().is_empty() => {
                Some(message.as_str())
            }
            _ => None,
        });
        let detail = reported
            .or_else(|| {
                stderr_lines
                    .iter()
                    .rev()
                    .find(|line| !line.trim().is_empty())
                    .map(String::as_str)
            })
            .unwrap_or("no diagnostics on stderr");
        bail!("collector failed ({status}): {detail}");
    }

    let usage_path = output_dir.join("usage.json");
    let document = read_document(&usage_path)?
        .with_context(|| format!("collector wrote no document at {}", usage_path.display()))?;

    Ok(RefreshOutcome {
        events,
        document,
        completed,
    })
}

pub fn output_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("usage")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_four_record_kinds() {
        assert_eq!(
            CollectorEvent::parse("phase\tcli\t2 days\t0.77"),
            Some(CollectorEvent::Phase {
                name: "cli".into(),
                detail: "2 days".into(),
                seconds: 0.77
            })
        );
        assert_eq!(
            CollectorEvent::parse("note\tdiscovering sources"),
            Some(CollectorEvent::Note {
                message: "discovering sources".into()
            })
        );
        assert_eq!(
            CollectorEvent::parse("summary\tsources\tcli, codex"),
            Some(CollectorEvent::Summary {
                name: "sources".into(),
                detail: "cli, codex".into()
            })
        );
        assert_eq!(
            CollectorEvent::parse("done\t4.07"),
            Some(CollectorEvent::Done { seconds: 4.07 })
        );
    }

    #[test]
    fn blank_lines_are_skipped_not_reported_as_unknown() {
        assert_eq!(CollectorEvent::parse(""), None);
        assert_eq!(CollectorEvent::parse("   \n"), None);
    }

    #[test]
    fn an_unrecognised_record_is_kept_verbatim() {
        assert_eq!(
            CollectorEvent::parse("futurekind\tdata"),
            Some(CollectorEvent::Unknown {
                line: "futurekind\tdata".into()
            })
        );
    }

    #[test]
    fn a_summary_containing_tabs_keeps_its_whole_detail() {
        assert_eq!(
            CollectorEvent::parse("summary\twindow\t23 days\t7 models"),
            Some(CollectorEvent::Summary {
                name: "window".into(),
                detail: "23 days\t7 models".into()
            })
        );
    }

    #[test]
    fn a_malformed_phase_time_degrades_to_zero_rather_than_failing() {
        assert_eq!(
            CollectorEvent::parse("phase\tcli\t2 days\tnot-a-number"),
            Some(CollectorEvent::Phase {
                name: "cli".into(),
                detail: "2 days".into(),
                seconds: 0.0
            })
        );
    }

    #[test]
    fn only_done_is_terminal() {
        assert!(CollectorEvent::Done { seconds: 1.0 }.is_terminal());
        assert!(!CollectorEvent::Note {
            message: String::new()
        }
        .is_terminal());
    }

    #[test]
    fn the_bundled_script_is_present_and_is_the_collector() {
        assert!(COLLECTOR_SCRIPT.starts_with("#!/usr/bin/env bash"));
        assert!(
            COLLECTOR_SCRIPT.contains(r#"| LC_ALL=C sort >"$TMP/cwds.txt""#),
            "the LC_ALL=C collation fix is missing from the bundled collector"
        );
        assert!(
            COLLECTOR_SCRIPT.contains(r#"| LC_ALL=C sort -u >"$TMP/cwds-have.txt""#),
            "the LC_ALL=C fix on cwds-have.txt is missing"
        );
    }

    #[test]
    fn a_missing_document_is_absent_not_an_error() {
        assert!(read_document(Path::new("/nonexistent/usage.json"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_future_schema_is_rejected_with_a_clear_message() {
        let dir = std::env::temp_dir().join(format!("veronica-usage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("usage.json");
        std::fs::write(&path, br#"{"schemaVersion": 99}"#).unwrap();
        let err = read_document(&path).unwrap_err().to_string();
        assert!(err.contains("schema version 99"), "got: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn outcome_exposes_summaries_and_errors_separately() {
        let outcome = RefreshOutcome {
            events: vec![
                CollectorEvent::Summary {
                    name: "spend".into(),
                    detail: "$179.78".into(),
                },
                CollectorEvent::Error {
                    message: "cursor auth missing".into(),
                },
            ],
            document: UsageDocument::default(),
            completed: true,
        };
        assert_eq!(outcome.summaries(), vec![("spend", "$179.78")]);
        assert_eq!(outcome.errors(), vec!["cursor auth missing"]);
    }

    #[tokio::test]
    async fn a_failed_run_reports_the_collectors_own_error_record() {
        let dir = std::env::temp_dir().join(format!("veronica-collector-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("collector");
        std::fs::write(
            &script,
            "printf 'note\\tdiscovering sources\\n'\nprintf 'error\\tno agent history\\n'\nexit 3\n",
        )
        .unwrap();
        let err = refresh(&script, &dir.join("out"), &dir.join("cache"), |_| {})
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("no agent history"), "got: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
