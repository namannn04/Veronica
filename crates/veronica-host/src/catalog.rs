use std::fs;
use std::io;

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use veronica_core::fsutil::write_atomic;
use veronica_protocol::catalog::{Catalog, CATALOG_URL};

use crate::fetch::{read_limited, Response, Transport};
use crate::layout::Layout;
use crate::trust::verify_envelope;

pub const SEED: &[u8] = include_bytes!("../seed/catalog.signed.json");
pub const MAX_ENVELOPE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogState {
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success: Option<String>,
}

impl CatalogState {
    pub fn last_success_at(&self) -> Option<DateTime<Utc>> {
        self.last_success
            .as_deref()
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .map(|time| time.with_timezone(&Utc))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    Seed,
    Cache,
    Network,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedCatalog {
    pub catalog: Catalog,
    pub provenance: Provenance,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("catalog revision {offered} is older than the trusted revision {floor}")]
pub struct RollbackError {
    pub offered: u64,
    pub floor: u64,
}

pub fn is_stale(catalog: &Catalog, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(&catalog.expires_at).map_or(true, |expires| expires < now)
}

pub fn seed() -> Catalog {
    verify_envelope(SEED).expect("the compiled-in seed catalog verifies")
}

#[derive(Debug, Clone)]
pub struct CatalogStore {
    layout: Layout,
}

impl CatalogStore {
    pub fn new(layout: Layout) -> Self {
        Self { layout }
    }

    pub fn state(&self) -> Result<CatalogState> {
        let path = self.layout.catalog_state_file();
        match fs::read(&path) {
            Ok(raw) => serde_json::from_slice(&raw)
                .with_context(|| format!("{} is not a valid catalog state", path.display())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(CatalogState::default()),
            Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
        }
    }

    fn save_state(&self, state: &CatalogState) -> Result<()> {
        let mut body = serde_json::to_vec_pretty(state)?;
        body.push(b'\n');
        write_atomic(&self.layout.catalog_state_file(), &body)
    }

    fn floor(state: &CatalogState, seed: &Catalog) -> u64 {
        state.revision.max(seed.revision)
    }

    fn cached(&self, floor: u64) -> Option<Catalog> {
        let raw = fs::read(self.layout.catalog_file()).ok()?;
        verify_envelope(&raw)
            .ok()
            .filter(|catalog| catalog.revision >= floor)
    }

    fn loaded(catalog: Catalog, provenance: Provenance, now: DateTime<Utc>) -> LoadedCatalog {
        LoadedCatalog {
            stale: is_stale(&catalog, now),
            catalog,
            provenance,
        }
    }

    pub fn load(&self, now: DateTime<Utc>) -> Result<LoadedCatalog> {
        let seed = seed();
        let floor = Self::floor(&self.state()?, &seed);
        Ok(match self.cached(floor) {
            Some(catalog) => Self::loaded(catalog, Provenance::Cache, now),
            None => Self::loaded(seed, Provenance::Seed, now),
        })
    }

    pub fn refresh(&self, transport: &dyn Transport, now: DateTime<Utc>) -> Result<LoadedCatalog> {
        let mut state = self.state()?;
        let floor = Self::floor(&state, &seed());
        let cached = self.cached(floor);
        let etag = state.etag.as_deref().filter(|_| cached.is_some());
        let stamp = Some(now.to_rfc3339_opts(SecondsFormat::Secs, true));
        match (transport.get(CATALOG_URL, etag)?, cached) {
            (Response::NotModified, Some(catalog)) => {
                state.last_success = stamp;
                self.save_state(&state)?;
                Ok(Self::loaded(catalog, Provenance::Cache, now))
            }
            (Response::NotModified, None) => {
                anyhow::bail!("the catalog server answered not modified without a cached copy")
            }
            (Response::Body { etag, body }, _) => {
                let raw = read_limited(body, MAX_ENVELOPE_BYTES)?;
                let catalog = verify_envelope(&raw)?;
                if catalog.revision < floor {
                    return Err(RollbackError {
                        offered: catalog.revision,
                        floor,
                    }
                    .into());
                }
                write_atomic(&self.layout.catalog_file(), &raw)?;
                self.save_state(&CatalogState {
                    revision: catalog.revision,
                    etag,
                    last_success: stamp,
                })?;
                Ok(Self::loaded(catalog, Provenance::Network, now))
            }
        }
    }
}
