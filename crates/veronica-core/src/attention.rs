//! Durable local state for Attention focus sessions.
//!
//! Edith keeps an active session separately from append-only history so a
//! crash never silently completes or loses a focus timer. Veronica mirrors
//! that model under the XDG data directory.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

const MIN_DURATION_SECONDS: i64 = 60;
const MAX_DURATION_SECONDS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttentionFocusSession {
    pub id: String,
    pub name: String,
    pub started_at: DateTime<Utc>,
    pub planned_duration_seconds: i64,
    pub ended_at: Option<DateTime<Utc>>,
}

impl AttentionFocusSession {
    pub fn planned_end(&self) -> DateTime<Utc> {
        self.started_at + Duration::seconds(self.planned_duration_seconds)
    }

    pub fn elapsed_seconds(&self, now: DateTime<Utc>) -> i64 {
        (now - self.started_at).num_seconds().max(0)
    }

    pub fn remaining_seconds(&self, now: DateTime<Utc>) -> i64 {
        self.planned_duration_seconds - self.elapsed_seconds(now)
    }

    pub fn actual_duration_seconds(&self, now: DateTime<Utc>) -> i64 {
        (self.ended_at.unwrap_or(now) - self.started_at)
            .num_seconds()
            .max(0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttentionStatus {
    pub active: Option<AttentionFocusSession>,
    pub completed_sessions: usize,
    pub total_focus_seconds: i64,
}

#[derive(Debug, Clone)]
pub struct AttentionRepository {
    root: PathBuf,
}

impl AttentionRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn active_file(&self) -> PathBuf {
        self.root.join("active-focus.json")
    }

    pub fn history_file(&self) -> PathBuf {
        self.root.join("focus.jsonl")
    }

    pub fn start_focus(
        &self,
        name: &str,
        duration_seconds: i64,
        now: DateTime<Utc>,
    ) -> Result<AttentionFocusSession> {
        if self.active_focus()?.is_some() {
            bail!("a focus session is already active");
        }
        let name = name.trim();
        if name.is_empty() {
            bail!("focus session name cannot be empty");
        }
        if !(MIN_DURATION_SECONDS..=MAX_DURATION_SECONDS).contains(&duration_seconds) {
            bail!("focus duration must be between 1 minute and 24 hours");
        }
        fs::create_dir_all(&self.root)
            .with_context(|| format!("cannot create {}", self.root.display()))?;
        let session = AttentionFocusSession {
            id: format!("{}-{}", now.timestamp_millis(), std::process::id()),
            name: name.to_string(),
            started_at: now,
            planned_duration_seconds: duration_seconds,
            ended_at: None,
        };
        atomic_json(&self.active_file(), &session)?;
        Ok(session)
    }

    pub fn active_focus(&self) -> Result<Option<AttentionFocusSession>> {
        let path = self.active_file();
        let body = match fs::read(&path) {
            Ok(body) => body,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| format!("cannot read {}", path.display()))
            }
        };
        let session = serde_json::from_slice(&body)
            .with_context(|| format!("invalid active focus session in {}", path.display()))?;
        Ok(Some(session))
    }

    pub fn stop_focus(&self, now: DateTime<Utc>) -> Result<AttentionFocusSession> {
        let mut session = self.active_focus()?.context("no focus session is active")?;
        session.ended_at = Some(now.max(session.started_at));
        fs::create_dir_all(&self.root)?;
        let line = serde_json::to_vec(&session)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.history_file())?;
        file.write_all(&line)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::remove_file(self.active_file())?;
        Ok(session)
    }

    pub fn history(&self) -> Result<Vec<AttentionFocusSession>> {
        let body = match fs::read_to_string(self.history_file()) {
            Ok(body) => body,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        body.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).context("invalid focus history record"))
            .collect()
    }

    pub fn status(&self, now: DateTime<Utc>) -> Result<AttentionStatus> {
        let history = self.history()?;
        Ok(AttentionStatus {
            active: self.active_focus()?,
            completed_sessions: history.len(),
            total_focus_seconds: history
                .iter()
                .map(|session| session.actual_duration_seconds(now))
                .sum(),
        })
    }
}

fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("focus state path has no parent")?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".focus-{}.tmp", std::process::id()));
    let body = serde_json::to_vec(value)?;
    {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(&body)?;
        file.sync_all()?;
    }
    fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(1);

    fn repository() -> AttentionRepository {
        let root = std::env::temp_dir().join(format!(
            "veronica-attention-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        AttentionRepository::new(root)
    }

    fn now() -> DateTime<Utc> {
        "2026-08-27T12:00:00Z".parse().unwrap()
    }

    #[test]
    fn session_survives_restart_and_moves_to_history() {
        let repository = repository();
        let started = repository.start_focus("Deep work", 2700, now()).unwrap();
        let reopened = AttentionRepository::new(repository.root.clone());
        assert_eq!(reopened.active_focus().unwrap(), Some(started));

        let stopped = reopened.stop_focus(now() + Duration::minutes(30)).unwrap();
        assert_eq!(stopped.actual_duration_seconds(now()), 1800);
        assert!(reopened.active_focus().unwrap().is_none());
        assert_eq!(reopened.history().unwrap(), vec![stopped]);
    }

    #[test]
    fn a_second_active_session_is_rejected() {
        let repository = repository();
        repository.start_focus("First", 60, now()).unwrap();
        assert!(repository.start_focus("Second", 60, now()).is_err());
    }

    #[test]
    fn invalid_names_and_durations_are_rejected() {
        let repository = repository();
        assert!(repository.start_focus("  ", 60, now()).is_err());
        assert!(repository.start_focus("Short", 59, now()).is_err());
        assert!(repository.start_focus("Long", 86_401, now()).is_err());
    }

    #[test]
    fn status_totals_completed_time_without_counting_active_time() {
        let repository = repository();
        repository.start_focus("One", 600, now()).unwrap();
        repository.stop_focus(now() + Duration::minutes(8)).unwrap();
        repository
            .start_focus("Two", 1200, now() + Duration::minutes(10))
            .unwrap();
        let status = repository.status(now() + Duration::minutes(15)).unwrap();
        assert_eq!(status.completed_sessions, 1);
        assert_eq!(status.total_focus_seconds, 480);
        assert_eq!(
            status
                .active
                .unwrap()
                .remaining_seconds(now() + Duration::minutes(15)),
            900
        );
    }
}
