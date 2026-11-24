use std::collections::{BTreeMap, BTreeSet};
use std::hash::{BuildHasher, Hasher};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use veronica_protocol::catalog::{Catalog, CatalogVersion, TargetArtifact};
use veronica_protocol::PackageVersion;

use crate::fetch::{self, Transport};
use crate::install::{HostIdentity, Installer};
use crate::registry::{InstalledExtension, Origin, Registry};

pub const DEFAULT_UPDATE_HOURS: u32 = 6;
pub const MIN_UPDATE_HOURS: u32 = 1;
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
    if installed.origin != Origin::Catalog {
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
    use chrono::{DateTime, Duration, Utc};

    use super::AutoUpdate;

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
