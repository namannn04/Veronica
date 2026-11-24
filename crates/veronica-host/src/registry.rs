use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use veronica_core::extensions::ENTRIES;
use veronica_core::fsutil::write_atomic;
use veronica_core::settings::Settings;
use veronica_protocol::PackageVersion;

pub const REGISTRY_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    Builtin,
    Catalog,
    Sideload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledExtension {
    pub version: PackageVersion,
    pub origin: Origin,
    pub enabled: bool,
    pub installed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<PackageVersion>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub quarantined: BTreeSet<PackageVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Registry {
    pub schema: u32,
    #[serde(default)]
    pub extensions: BTreeMap<String, InstalledExtension>,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            schema: REGISTRY_SCHEMA,
            extensions: BTreeMap::new(),
        }
    }
}

impl Registry {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = match std::fs::read(path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(error) => {
                return Err(error).with_context(|| format!("cannot read {}", path.display()))
            }
        };
        let registry: Self = serde_json::from_slice(&raw)
            .with_context(|| format!("{} is not a valid registry", path.display()))?;
        anyhow::ensure!(
            registry.schema == REGISTRY_SCHEMA,
            "{} has unsupported schema {}",
            path.display(),
            registry.schema
        );
        Ok(registry)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let mut body = serde_json::to_vec_pretty(self)?;
        body.push(b'\n');
        write_atomic(path, &body)
    }

    pub fn get(&self, id: &str) -> Option<&InstalledExtension> {
        self.extensions.get(id)
    }

    #[must_use]
    pub fn with_builtins(mut self, settings: &Settings) -> Self {
        let version: PackageVersion = env!("CARGO_PKG_VERSION")
            .parse()
            .expect("the host version is a package version");
        for entry in ENTRIES {
            self.extensions
                .entry(entry.id.to_string())
                .or_insert_with(|| InstalledExtension {
                    version,
                    origin: Origin::Builtin,
                    enabled: settings.extension_enabled(entry),
                    installed_at: String::new(),
                    previous: None,
                    quarantined: BTreeSet::new(),
                });
        }
        self
    }
}

pub fn view(installed: &Path, settings: &Settings) -> Result<Registry> {
    Ok(Registry::load(installed)?.with_builtins(settings))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use veronica_core::extensions::ENTRIES;
    use veronica_core::settings::Settings;

    use super::{view, Origin, Registry};

    #[test]
    fn registry_round_trips_through_disk() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("state/installed.json");
        assert_eq!(Registry::load(&path).unwrap(), Registry::default());
        let raw = json!({
            "schema": 1,
            "extensions": {
                "media": {
                    "version": "1.0.7", "origin": "catalog", "enabled": true,
                    "installedAt": "2026-11-24T12:00:00Z", "previous": "1.0.6",
                    "quarantined": ["1.0.5"]
                },
                "probe": {
                    "version": "0.1.0", "origin": "sideload", "enabled": false,
                    "installedAt": "2026-11-24T12:00:00Z"
                }
            }
        });
        let registry: Registry = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(registry.get("probe").unwrap().origin, Origin::Sideload);
        registry.save(&path).unwrap();
        let loaded = Registry::load(&path).unwrap();
        assert_eq!(loaded, registry);
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&text).unwrap(),
            raw
        );
    }

    #[test]
    fn unknown_schemas_and_fields_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("installed.json");
        std::fs::write(&path, r#"{"schema":2,"extensions":{}}"#).unwrap();
        assert!(Registry::load(&path).is_err());
        std::fs::write(&path, r#"{"schema":1,"extra":true}"#).unwrap();
        assert!(Registry::load(&path).is_err());
    }

    #[test]
    fn the_view_lists_every_built_in_feature_without_shadowing_installed_ones() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("installed.json");
        let shadowed = ENTRIES[0].id;
        let raw = serde_json::json!({
            "schema": 1,
            "extensions": {
                shadowed: {
                    "version": "2.0.0", "origin": "catalog", "enabled": false,
                    "installedAt": "2026-11-24T12:00:00Z"
                }
            }
        });
        std::fs::write(&path, raw.to_string()).unwrap();
        let mut settings = Settings::default();
        settings.set(ENTRIES[1].defaults_key, serde_json::Value::Bool(true));
        let registry = view(&path, &settings).unwrap();
        assert_eq!(registry.extensions.len(), ENTRIES.len());
        assert_eq!(registry.get(shadowed).unwrap().origin, Origin::Catalog);
        let builtin = registry.get(ENTRIES[1].id).unwrap();
        assert_eq!(builtin.origin, Origin::Builtin);
        assert!(builtin.enabled);
        assert_eq!(builtin.version.to_string(), env!("CARGO_PKG_VERSION"));
        assert_eq!(Registry::load(&path).unwrap().extensions.len(), 1);
    }
}
