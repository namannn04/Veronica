use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use veronica_core::Settings;
use veronica_host::bundled::{claim_due, Bundled};
use veronica_host::catalog::CatalogStore;
use veronica_host::fetch::HttpTransport;
use veronica_host::update::{plan, AutoUpdate, Decision, Trigger};
use veronica_host::HostIdentity;

use super::host::Host;
use super::jobs::{submit, Action, Kind};

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
        let stored = AutoUpdate::from_settings(settings);
        Self {
            enabled: stored.enabled,
            interval_hours: stored.interval_hours,
            disabled: stored.disabled,
        }
    }

    pub(crate) fn write(&self, settings: &mut Settings) {
        self.planner().write_settings(settings);
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

pub(crate) async fn install_bundled(host: &Arc<Host>, settings: Settings) {
    let installer = host.installer();
    let due = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
        match Bundled::current()? {
            Some(bundled) => claim_due(&installer, &bundled, &settings),
            None => Ok(Vec::new()),
        }
    })
    .await
    .map_err(anyhow::Error::from)
    .and_then(|due| due);
    match due {
        Ok(ids) => {
            for id in ids {
                if !host.jobs().is_busy(&id) {
                    submit(host, Action::on(Kind::Install, id));
                }
            }
        }
        Err(error) => {
            tracing::warn!(target: "veronica", "cannot install the bundled extensions: {error:#}");
        }
    }
}

pub(crate) async fn run(host: Arc<Host>, settings_file: PathBuf) {
    let mut jitter = AutoUpdate::default().random_jitter();
    let mut attempted = None;
    tokio::time::sleep(FIRST_LOOK).await;
    loop {
        let settings = Settings::load(&settings_file)
            .map(|settings| AutoUpdate::from_settings(&settings))
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
