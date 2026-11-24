use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use veronica_core::Settings;
use veronica_host::catalog::CatalogStore;
use veronica_host::fetch::HttpTransport;
use veronica_host::update::{
    plan, AutoUpdate, Decision, Trigger, DEFAULT_UPDATE_HOURS, MIN_UPDATE_HOURS,
};
use veronica_host::HostIdentity;

use super::host::Host;
use super::jobs::{submit, Action, Kind};

const ENABLED: &str = "extensionsAutoUpdate";
const HOURS: &str = "extensionsUpdateHours";
const OFF: &str = "extensionsAutoUpdateOff";
const FIRST_LOOK: Duration = Duration::from_mins(1);
const LOOK_EVERY: Duration = Duration::from_mins(5);
const RETRY_AFTER: chrono::Duration = chrono::Duration::hours(1);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Preferences {
    pub enabled: bool,
    pub interval_hours: u32,
    pub disabled: BTreeSet<String>,
}

impl Preferences {
    pub(crate) fn read(settings: &Settings) -> Self {
        let hours = settings
            .get(HOURS)
            .and_then(Value::as_u64)
            .and_then(|hours| u32::try_from(hours).ok())
            .unwrap_or(DEFAULT_UPDATE_HOURS);
        let disabled = settings
            .get(OFF)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|id| id.as_str().map(String::from))
            .collect();
        Self {
            enabled: settings.bool_or(ENABLED, true),
            interval_hours: hours.max(MIN_UPDATE_HOURS),
            disabled,
        }
    }

    pub(crate) fn write(&self, settings: &mut Settings) {
        settings.set(ENABLED, json!(self.enabled));
        settings.set(HOURS, json!(self.interval_hours.max(MIN_UPDATE_HOURS)));
        settings.set(OFF, json!(self.disabled));
    }

    fn planner(&self) -> AutoUpdate {
        AutoUpdate {
            enabled: self.enabled,
            interval_hours: self.interval_hours,
            disabled: self.disabled.clone(),
        }
    }
}

pub(crate) fn ready(
    decisions: &BTreeMap<String, Decision>,
    held_back: impl Fn(&str) -> bool,
) -> Vec<String> {
    decisions
        .iter()
        .filter(|(id, decision)| matches!(decision, Decision::Update { .. }) && !held_back(id))
        .map(|(id, _)| id.clone())
        .collect()
}

fn pending(host: &Host, settings: &AutoUpdate) -> Result<Vec<String>> {
    let catalog = CatalogStore::new(host.layout().clone())
        .load(Utc::now())?
        .catalog;
    let decisions = plan(
        &host.registry()?,
        &catalog,
        &HostIdentity::current(),
        settings,
        Trigger::Automatic,
    );
    Ok(ready(&decisions, |id| {
        !host.open_pages(id).is_empty() || host.jobs().is_busy(id)
    }))
}

async fn refresh(host: &Arc<Host>) -> Result<()> {
    let store = CatalogStore::new(host.layout().clone());
    tokio::task::spawn_blocking(move || store.refresh(&HttpTransport::new()?, Utc::now()).map(drop))
        .await?
}

pub(crate) async fn run(host: Arc<Host>, settings_file: PathBuf) {
    let mut jitter = AutoUpdate::default().random_jitter();
    let mut attempted = None;
    tokio::time::sleep(FIRST_LOOK).await;
    loop {
        let settings = Settings::load(&settings_file)
            .map(|settings| Preferences::read(&settings).planner())
            .unwrap_or_default();
        let now = Utc::now();
        let checked = CatalogStore::new(host.layout().clone())
            .state()
            .ok()
            .and_then(|state| state.last_success_at());
        let retry = attempted.is_none_or(|last| now >= last + RETRY_AFTER);
        if settings.is_due(checked, now, jitter) && retry {
            attempted = Some(now);
            jitter = settings.random_jitter();
            if let Err(error) = refresh(&host).await {
                tracing::info!(target: "veronica", "cannot refresh the extension catalog: {error:#}");
            }
        }
        match pending(&host, &settings) {
            Ok(ids) => {
                for id in ids {
                    submit(&host, Action::on(Kind::Update, id));
                }
            }
            Err(error) => {
                tracing::warn!(target: "veronica", "cannot plan extension updates: {error:#}");
            }
        }
        tokio::time::sleep(LOOK_EVERY).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_default_to_six_hours_and_round_trip_through_settings() {
        let mut settings = Settings::default();
        let defaults = Preferences::read(&settings);
        assert_eq!((defaults.enabled, defaults.interval_hours), (true, 6));
        assert!(defaults.disabled.is_empty());

        let chosen = Preferences {
            enabled: false,
            interval_hours: 0,
            disabled: BTreeSet::from(["media".to_string()]),
        };
        chosen.write(&mut settings);
        let stored = Preferences::read(&settings);
        assert_eq!(stored.interval_hours, 1);
        assert_eq!(
            (stored.enabled, &stored.disabled),
            (false, &chosen.disabled)
        );
        assert!(!stored.planner().allows("media"));

        settings.set(HOURS, json!("often"));
        settings.set(OFF, json!("media"));
        let odd = Preferences::read(&settings);
        assert_eq!(odd.interval_hours, 6);
        assert!(odd.disabled.is_empty());
    }

    #[test]
    fn updates_wait_while_a_page_is_open() {
        let version = |raw: &str| raw.parse().unwrap();
        let update = Decision::Update {
            from: version("1.0.1"),
            to: version("1.0.2"),
        };
        let decisions = BTreeMap::from([
            ("calendar".to_string(), update),
            ("media".to_string(), update),
            (
                "usage".to_string(),
                Decision::Skip(veronica_host::update::Skip::UpToDate),
            ),
        ]);
        assert_eq!(ready(&decisions, |id| id == "media"), ["calendar"]);
        assert_eq!(ready(&decisions, |_| false), ["calendar", "media"]);
    }
}
