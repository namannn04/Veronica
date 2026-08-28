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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AttentionPrivacy {
    Applications,
    Detailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttentionCategory {
    pub id: String,
    pub name: String,
    pub color: String,
    pub applications: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct AttentionSettings {
    pub enabled: bool,
    pub idle_threshold_seconds: i64,
    pub privacy: AttentionPrivacy,
    pub categories: Vec<AttentionCategory>,
}

impl Default for AttentionSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            idle_threshold_seconds: 300,
            privacy: AttentionPrivacy::Applications,
            categories: vec![
                AttentionCategory {
                    id: "focus".into(),
                    name: "Focus".into(),
                    color: "#55755f".into(),
                    applications: vec![
                        "code".into(),
                        "codex".into(),
                        "terminal".into(),
                        "jetbrains".into(),
                    ],
                },
                AttentionCategory {
                    id: "communication".into(),
                    name: "Communication".into(),
                    color: "#2a78d6".into(),
                    applications: vec![
                        "slack".into(),
                        "discord".into(),
                        "teams".into(),
                        "zoom".into(),
                    ],
                },
                AttentionCategory {
                    id: "entertainment".into(),
                    name: "Entertainment".into(),
                    color: "#d97757".into(),
                    applications: vec!["spotify".into(), "vlc".into(), "steam".into()],
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttentionEvent {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub duration_seconds: i64,
    pub application: String,
    pub title: Option<String>,
    pub idle: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AttentionOverview {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub active_seconds: i64,
    pub idle_seconds: i64,
    pub focused_seconds: i64,
    pub context_switches: usize,
    pub applications: Vec<(String, i64)>,
    pub categories: Vec<(String, i64)>,
    pub events: Vec<AttentionEvent>,
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

    fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }
    fn events_file(&self) -> PathBuf {
        self.root.join("events.jsonl")
    }

    pub fn load_settings(&self) -> Result<AttentionSettings> {
        match fs::read(self.settings_file()) {
            Ok(body) => Ok(serde_json::from_slice(&body).context("invalid Attention settings")?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(AttentionSettings::default())
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn save_settings(&self, settings: &AttentionSettings) -> Result<()> {
        if !(30..=3600).contains(&settings.idle_threshold_seconds) {
            bail!("idle threshold must be between 30 seconds and 1 hour");
        }
        atomic_json(&self.settings_file(), settings)
    }

    pub fn record(
        &self,
        application: &str,
        title: Option<&str>,
        idle: bool,
        duration_seconds: i64,
        now: DateTime<Utc>,
    ) -> Result<Option<AttentionEvent>> {
        let settings = self.load_settings()?;
        if !settings.enabled {
            return Ok(None);
        }
        let application = application.trim();
        if application.is_empty() || !(1..=120).contains(&duration_seconds) {
            return Ok(None);
        }
        fs::create_dir_all(&self.root)?;
        let event = AttentionEvent {
            id: format!("{}-{}", now.timestamp_millis(), std::process::id()),
            started_at: now - Duration::seconds(duration_seconds),
            duration_seconds,
            application: application.chars().take(120).collect(),
            title: if settings.privacy == AttentionPrivacy::Detailed {
                title.map(|value| value.chars().take(240).collect())
            } else {
                None
            },
            idle,
        };
        let mut events = self.events()?;
        if let Some(last) = events.last_mut() {
            let gap = (event.started_at
                - (last.started_at + Duration::seconds(last.duration_seconds)))
            .num_seconds()
            .abs();
            if gap <= 3
                && last.application == event.application
                && last.title == event.title
                && last.idle == event.idle
            {
                last.duration_seconds += event.duration_seconds;
                let merged = last.clone();
                self.write_events(&events)?;
                return Ok(Some(merged));
            }
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.events_file())?;
        file.write_all(&serde_json::to_vec(&event)?)?;
        file.write_all(b"\n")?;
        Ok(Some(event))
    }

    pub fn events(&self) -> Result<Vec<AttentionEvent>> {
        let body = match fs::read_to_string(self.events_file()) {
            Ok(body) => body,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        body.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).context("invalid Attention event"))
            .collect()
    }

    fn write_events(&self, events: &[AttentionEvent]) -> Result<()> {
        let body = events
            .iter()
            .map(serde_json::to_string)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join("\n")
            + "\n";
        let path = self.events_file();
        let temporary = self
            .root
            .join(format!(".events-{}.tmp", std::process::id()));
        fs::write(&temporary, body)?;
        fs::rename(temporary, path)?;
        Ok(())
    }

    pub fn overview(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<AttentionOverview> {
        let settings = self.load_settings()?;
        let events: Vec<_> = self
            .events()?
            .into_iter()
            .filter(|event| {
                event.started_at < to
                    && event.started_at + Duration::seconds(event.duration_seconds) > from
            })
            .collect();
        let mut apps = std::collections::HashMap::<String, i64>::new();
        let mut categories = std::collections::HashMap::<String, i64>::new();
        let mut active = 0;
        let mut idle = 0;
        let mut focused = 0;
        let mut switches = 0;
        let mut previous = None;
        for event in &events {
            if event.idle {
                idle += event.duration_seconds;
                continue;
            }
            active += event.duration_seconds;
            *apps.entry(event.application.clone()).or_default() += event.duration_seconds;
            if previous.as_ref() != Some(&event.application) && previous.is_some() {
                switches += 1;
            }
            previous = Some(event.application.clone());
            let lower = event.application.to_lowercase();
            let category = settings
                .categories
                .iter()
                .find(|category| {
                    category
                        .applications
                        .iter()
                        .any(|pattern| lower.contains(&pattern.to_lowercase()))
                })
                .map(|category| category.id.clone())
                .unwrap_or_else(|| "unclassified".into());
            *categories.entry(category.clone()).or_default() += event.duration_seconds;
            if category == "focus" {
                focused += event.duration_seconds;
            }
        }
        let sort = |map: std::collections::HashMap<String, i64>| {
            let mut rows: Vec<_> = map.into_iter().collect();
            rows.sort_by_key(|row| std::cmp::Reverse(row.1));
            rows
        };
        Ok(AttentionOverview {
            from,
            to,
            active_seconds: active,
            idle_seconds: idle,
            focused_seconds: focused,
            context_switches: switches,
            applications: sort(apps),
            categories: sort(categories),
            events,
        })
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

    #[test]
    fn disabled_tracking_records_nothing_and_application_privacy_drops_titles() {
        let repository = repository();
        assert!(repository
            .record("Code", Some("secret.rs"), false, 10, now())
            .unwrap()
            .is_none());
        let mut settings = repository.load_settings().unwrap();
        settings.enabled = true;
        repository.save_settings(&settings).unwrap();
        let event = repository
            .record("Code", Some("secret.rs"), false, 10, now())
            .unwrap()
            .unwrap();
        assert_eq!(event.title, None);
    }

    #[test]
    fn consecutive_pulses_merge_and_summary_classifies_them() {
        let repository = repository();
        let mut settings = repository.load_settings().unwrap();
        settings.enabled = true;
        repository.save_settings(&settings).unwrap();
        repository
            .record("Visual Studio Code", None, false, 10, now())
            .unwrap();
        repository
            .record(
                "Visual Studio Code",
                None,
                false,
                10,
                now() + Duration::seconds(10),
            )
            .unwrap();
        repository
            .record("Away", None, true, 5, now() + Duration::seconds(15))
            .unwrap();
        let events = repository.events().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].duration_seconds, 20);
        let overview = repository
            .overview(now() - Duration::minutes(1), now() + Duration::minutes(1))
            .unwrap();
        assert_eq!(overview.active_seconds, 20);
        assert_eq!(overview.idle_seconds, 5);
        assert_eq!(overview.focused_seconds, 20);
    }

    #[test]
    fn detailed_privacy_keeps_a_bounded_title() {
        let repository = repository();
        let mut settings = repository.load_settings().unwrap();
        settings.enabled = true;
        settings.privacy = AttentionPrivacy::Detailed;
        repository.save_settings(&settings).unwrap();
        let event = repository
            .record("Terminal", Some(&"x".repeat(300)), false, 10, now())
            .unwrap()
            .unwrap();
        assert_eq!(event.title.unwrap().chars().count(), 240);
    }
}
