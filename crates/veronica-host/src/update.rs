use std::collections::{BTreeMap, BTreeSet};
use std::hash::{BuildHasher, Hasher};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use veronica_core::settings::Settings;
use veronica_protocol::catalog::{Catalog, CatalogVersion, TargetArtifact};
use veronica_protocol::PackageVersion;

use crate::fetch::{self, Transport};
use crate::install::{HostIdentity, Installer};
use crate::registry::{InstalledExtension, Origin, Registry};

pub const DEFAULT_UPDATE_HOURS: u32 = 6;
pub const MIN_UPDATE_HOURS: u32 = 1;
pub const AUTO_UPDATE_KEY: &str = "extensionsAutoUpdate";
pub const UPDATE_HOURS_KEY: &str = "extensionsUpdateHours";
pub const AUTO_UPDATE_OFF_KEY: &str = "extensionsAutoUpdateOff";
const JITTER_DIVISOR: i32 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoUpdate {
    pub enabled: bool,
    pub interval_hours: u32,
    pub disabled: BTreeSet<String>,
}

impl Default for AutoUpdate {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_hours: DEFAULT_UPDATE_HOURS,
            disabled: BTreeSet::new(),
        }
    }
}

impl AutoUpdate {
    pub fn from_settings(settings: &Settings) -> Self {
        let hours = settings
            .get(UPDATE_HOURS_KEY)
            .and_then(Value::as_u64)
            .and_then(|hours| u32::try_from(hours).ok())
            .unwrap_or(DEFAULT_UPDATE_HOURS);
        let disabled = settings
            .get(AUTO_UPDATE_OFF_KEY)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|id| id.as_str().map(String::from))
            .collect();
        Self {
            enabled: settings.bool_or(AUTO_UPDATE_KEY, true),
            interval_hours: hours.max(MIN_UPDATE_HOURS),
            disabled,
        }
    }

    pub fn write_settings(&self, settings: &mut Settings) {
        settings.set(AUTO_UPDATE_KEY, json!(self.enabled));
        settings.set(
            UPDATE_HOURS_KEY,
            json!(self.interval_hours.max(MIN_UPDATE_HOURS)),
        );
        settings.set(AUTO_UPDATE_OFF_KEY, json!(self.disabled));
    }

    pub fn interval(&self) -> Duration {
        Duration::hours(self.interval_hours.max(MIN_UPDATE_HOURS).into())
    }

    pub fn max_jitter(&self) -> Duration {
        self.interval() / JITTER_DIVISOR
    }

    pub fn random_jitter(&self) -> Duration {
        let span = u64::try_from(self.max_jitter().num_seconds())
            .unwrap_or(0)
            .max(1);
        let random = std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish();
        Duration::seconds(i64::try_from(random % span).unwrap_or(0))
    }

    pub fn is_due(
        &self,
        last_check: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
        jitter: Duration,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        let jitter = jitter.clamp(Duration::zero(), self.max_jitter());
        last_check.is_none_or(|last| now >= last + self.interval() + jitter)
    }

    pub fn allows(&self, id: &str) -> bool {
        self.enabled && !self.disabled.contains(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Manual,
    Automatic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    NotFromCatalog,
    AutoUpdateOff,
    NotInCatalog,
    NoCompatibleVersion,
    UpToDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Update {
        from: PackageVersion,
        to: PackageVersion,
    },
    Skip(Skip),
}

pub fn resolve<'a>(
    catalog: &'a Catalog,
    id: &str,
    host: &HostIdentity,
    quarantined: &BTreeSet<PackageVersion>,
) -> Option<(&'a CatalogVersion, &'a TargetArtifact)> {
    let extension = catalog.extension(id)?;
    let mut versions: Vec<_> = extension.versions.iter().collect();
    versions.sort_by_key(|entry| std::cmp::Reverse(entry.version));
    versions.into_iter().find_map(|entry| {
        let usable = !entry.yanked
            && !quarantined.contains(&entry.version)
            && host.check_engines(id, &entry.engines).is_ok();
        usable
            .then(|| entry.artifact(host.target))
            .flatten()
            .map(|artifact| (entry, artifact))
    })
}

fn decide(
    id: &str,
    installed: &InstalledExtension,
    catalog: &Catalog,
    host: &HostIdentity,
    settings: &AutoUpdate,
    trigger: Trigger,
) -> Decision {
    if !matches!(installed.origin, Origin::Catalog | Origin::Bundled) {
        return Decision::Skip(Skip::NotFromCatalog);
    }
    if trigger == Trigger::Automatic && !settings.allows(id) {
        return Decision::Skip(Skip::AutoUpdateOff);
    }
    if catalog.extension(id).is_none() {
        return Decision::Skip(Skip::NotInCatalog);
    }
    match resolve(catalog, id, host, &installed.quarantined) {
        None => Decision::Skip(Skip::NoCompatibleVersion),
        Some((entry, _)) if entry.version > installed.version => Decision::Update {
            from: installed.version,
            to: entry.version,
        },
        Some(_) => Decision::Skip(Skip::UpToDate),
    }
}

pub fn plan(
    registry: &Registry,
    catalog: &Catalog,
    host: &HostIdentity,
    settings: &AutoUpdate,
    trigger: Trigger,
) -> BTreeMap<String, Decision> {
    registry
        .extensions
        .iter()
        .map(|(id, installed)| {
            let decision = decide(id, installed, catalog, host, settings, trigger);
            (id.clone(), decision)
        })
        .collect()
}

pub fn install_from_catalog(
    installer: &Installer,
    catalog: &Catalog,
    transport: &dyn Transport,
    id: &str,
) -> Result<InstalledExtension> {
    anyhow::ensure!(
        catalog.extension(id).is_some(),
        "{id} is not in the catalog"
    );
    let quarantined = installer
        .registry()?
        .get(id)
        .map(|entry| entry.quarantined.clone())
        .unwrap_or_default();
    let (entry, artifact) = resolve(catalog, id, installer.host(), &quarantined)
        .with_context(|| format!("no published version of {id} fits this host"))?;
    let download = fetch::download(installer.layout(), transport, artifact)?;
    installer.install_archive(download.open()?, id, entry.version)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::{DateTime, Duration, Utc};
    use serde_json::json;
    use veronica_core::settings::Settings;

    use super::{AutoUpdate, AUTO_UPDATE_OFF_KEY, UPDATE_HOURS_KEY};

    #[test]
    fn preferences_default_to_six_hours_and_round_trip_through_settings() {
        let mut settings = Settings::default();
        assert_eq!(AutoUpdate::from_settings(&settings), AutoUpdate::default());
        let chosen = AutoUpdate {
            enabled: false,
            interval_hours: 0,
            disabled: BTreeSet::from(["media".to_string()]),
        };
        chosen.write_settings(&mut settings);
        let stored = AutoUpdate::from_settings(&settings);
        assert_eq!(stored.interval_hours, 1);
        assert_eq!(
            (stored.enabled, &stored.disabled),
            (false, &chosen.disabled)
        );
        assert!(!stored.allows("media"));
        settings.set(UPDATE_HOURS_KEY, json!("often"));
        settings.set(AUTO_UPDATE_OFF_KEY, json!("media"));
        let odd = AutoUpdate::from_settings(&settings);
        assert_eq!(odd.interval_hours, 6);
        assert!(odd.disabled.is_empty());
    }

    fn at(raw: &str) -> DateTime<Utc> {
        raw.parse().unwrap()
    }

    #[test]
    fn checks_fall_due_after_the_interval_plus_jitter() {
        let settings = AutoUpdate::default();
        let last = at("2026-11-24T00:00:00Z");
        assert!(settings.is_due(None, last, Duration::zero()));
        assert!(!settings.is_due(Some(last), at("2026-11-24T05:59:59Z"), Duration::zero()));
        assert!(settings.is_due(Some(last), at("2026-11-24T06:00:00Z"), Duration::zero()));
        let jitter = Duration::minutes(20);
        assert!(!settings.is_due(Some(last), at("2026-11-24T06:19:59Z"), jitter));
        assert!(settings.is_due(Some(last), at("2026-11-24T06:20:00Z"), jitter));
        assert!(settings.is_due(Some(last), at("2026-11-24T06:36:00Z"), Duration::days(9)));
    }

    #[test]
    fn the_interval_has_a_floor_and_jitter_stays_bounded() {
        let settings = AutoUpdate {
            interval_hours: 0,
            ..AutoUpdate::default()
        };
        assert_eq!(settings.interval(), Duration::hours(1));
        assert_eq!(settings.max_jitter(), Duration::minutes(6));
        for _ in 0..64 {
            let jitter = settings.random_jitter();
            assert!(jitter >= Duration::zero() && jitter < settings.max_jitter());
        }
        let off = AutoUpdate {
            enabled: false,
            ..AutoUpdate::default()
        };
        assert!(!off.is_due(None, at("2026-11-24T00:00:00Z"), Duration::zero()));
    }
}
