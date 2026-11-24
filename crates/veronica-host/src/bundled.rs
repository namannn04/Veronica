use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use veronica_core::extensions::ENTRIES;
use veronica_core::fsutil::write_atomic;
use veronica_core::packaging::Packaging;
use veronica_core::settings::Settings;
use veronica_protocol::bundled::{
    BundledIndex, BundledPackage, BUNDLED_INDEX, BUNDLED_SCHEMA, NEVER_BUNDLED,
};
use veronica_protocol::catalog::Catalog;
use veronica_protocol::manifest::Profile;

use crate::archive::sha256_hex;
use crate::fetch::Transport;
use crate::install::Installer;
use crate::launcher::LaunchEnvironment;
use crate::registry::{InstalledExtension, Origin, Registry};
use crate::update::{install_from_catalog, resolve};

pub const DEB_DIR: &str = "/usr/share/veronica/extensions";
pub const FLATPAK_DIR: &str = "/app/share/veronica/extensions";
pub const PACKAGED_DIR: &str = "usr/share/veronica/extensions";
pub const PROFILE_KEY: &str = "profile";

const LEGACY_IDS: [(&str, &str); 4] = [
    ("music", "media"),
    ("seoAudit", "audit"),
    ("appMaintenance", "maintenance"),
    ("colorPicker", "color"),
];

pub fn bundled_dir(environment: &LaunchEnvironment) -> Option<PathBuf> {
    match environment.packaging {
        Packaging::Deb => Some(PathBuf::from(DEB_DIR)),
        Packaging::Flatpak => Some(PathBuf::from(FLATPAK_DIR)),
        Packaging::Snap => environment
            .snap
            .as_ref()
            .map(|root| root.join(PACKAGED_DIR)),
        Packaging::AppImage => environment
            .appdir
            .as_ref()
            .map(|root| root.join(PACKAGED_DIR)),
    }
}

pub fn extension_id(legacy: &str) -> &str {
    LEGACY_IDS
        .iter()
        .find(|(from, _)| *from == legacy)
        .map_or(legacy, |(_, to)| to)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundled {
    dir: PathBuf,
    index: BundledIndex,
}

impl Bundled {
    pub fn current() -> Result<Option<Self>> {
        bundled_dir(&LaunchEnvironment::current()).map_or(Ok(None), |dir| Self::open(&dir))
    }

    pub fn open(dir: &Path) -> Result<Option<Self>> {
        let path = dir.join(BUNDLED_INDEX);
        let raw = match std::fs::read(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| format!("cannot read {}", path.display()))
            }
        };
        let index: BundledIndex = serde_json::from_slice(&raw)
            .with_context(|| format!("{} is not a bundled index", path.display()))?;
        ensure!(
            index.schema == BUNDLED_SCHEMA,
            "{} has unsupported schema {}",
            path.display(),
            index.schema
        );
        if let Some(test) = index
            .extensions
            .iter()
            .find(|package| NEVER_BUNDLED.contains(&package.id.as_str()))
        {
            bail!("{} bundles the test extension {}", path.display(), test.id);
        }
        Ok(Some(Self {
            dir: dir.to_path_buf(),
            index,
        }))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn index(&self) -> &BundledIndex {
        &self.index
    }

    pub fn get(&self, id: &str) -> Option<&BundledPackage> {
        self.index.get(id)
    }

    pub fn verified(&self, id: &str) -> Result<(&BundledPackage, Vec<u8>)> {
        let package = self
            .get(id)
            .with_context(|| format!("{id} is not bundled with this app"))?;
        let path = self.dir.join(package.file_name());
        let bytes =
            std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
        ensure!(
            bytes.len() as u64 == package.size && sha256_hex(bytes.as_slice())? == package.sha256,
            "{} does not match the bundled index",
            path.display()
        );
        Ok((package, bytes))
    }

    pub fn install(&self, installer: &Installer, id: &str) -> Result<InstalledExtension> {
        let (package, bytes) = self.verified(id)?;
        installer.install_bundled(bytes.as_slice(), id, package.version)
    }

    pub fn wanted(&self, settings: &Settings) -> BTreeSet<String> {
        let profile: Option<Profile> = settings
            .get(PROFILE_KEY)
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok());
        let legacy_user = ENTRIES
            .iter()
            .any(|entry| settings.get(entry.defaults_key).is_some());
        let toggled: BTreeSet<&str> = ENTRIES
            .iter()
            .filter(|entry| {
                let explicit = settings.get(entry.defaults_key) == Some(&Value::Bool(true));
                explicit || (legacy_user && settings.extension_enabled(entry))
            })
            .map(|entry| extension_id(entry.id))
            .collect();
        self.index
            .extensions
            .iter()
            .filter(|package| {
                profile.is_some_and(|profile| package.profiles.contains(&profile))
                    || toggled.contains(package.id.as_str())
            })
            .map(|package| package.id.clone())
            .collect()
    }

    pub fn due(
        &self,
        registry: &Registry,
        settings: &Settings,
        offered: &BTreeSet<String>,
    ) -> Vec<String> {
        let wanted = self.wanted(settings);
        self.index
            .extensions
            .iter()
            .filter(|package| match registry.get(&package.id) {
                None => wanted.contains(&package.id) && !offered.contains(&package.id),
                Some(entry) => {
                    matches!(entry.origin, Origin::Bundled | Origin::Catalog)
                        && package.version > entry.version
                        && !entry.quarantined.contains(&package.version)
                }
            })
            .map(|package| package.id.clone())
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Offered {
    offered: BTreeSet<String>,
}

fn load_offered(path: &Path) -> Result<BTreeSet<String>> {
    match std::fs::read(path) {
        Ok(raw) => Ok(serde_json::from_slice::<Offered>(&raw)
            .with_context(|| format!("{} is invalid", path.display()))?
            .offered),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeSet::new()),
        Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
    }
}

pub fn claim_due(
    installer: &Installer,
    bundled: &Bundled,
    settings: &Settings,
) -> Result<Vec<String>> {
    let path = installer.layout().bundled_file();
    let mut offered = load_offered(&path)?;
    let due = bundled.due(&installer.registry()?, settings, &offered);
    if due.iter().any(|id| !offered.contains(id)) {
        offered.extend(due.iter().cloned());
        let mut body = serde_json::to_vec_pretty(&Offered { offered })?;
        body.push(b'\n');
        write_atomic(&path, &body)?;
    }
    Ok(due)
}

pub fn install_preferred(
    installer: &Installer,
    catalog: &Catalog,
    bundled: Option<&Bundled>,
    transport: &dyn Transport,
    id: &str,
) -> Result<InstalledExtension> {
    let quarantined = installer
        .registry()?
        .get(id)
        .map(|entry| entry.quarantined.clone())
        .unwrap_or_default();
    let packaged = bundled.filter(|bundled| {
        bundled
            .get(id)
            .is_some_and(|package| !quarantined.contains(&package.version))
    });
    let Some(packaged) = packaged else {
        return install_from_catalog(installer, catalog, transport, id);
    };
    let bundled_version = packaged.get(id).map(|package| package.version);
    let published =
        resolve(catalog, id, installer.host(), &quarantined).map(|(entry, _)| entry.version);
    if published > bundled_version {
        if let Ok(entry) = install_from_catalog(installer, catalog, transport, id) {
            return Ok(entry);
        }
    }
    packaged.install(installer, id)
}
