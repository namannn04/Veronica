use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use veronica_core::extensions::{ENTRIES, LEGACY_TOGGLES};
use veronica_core::fsutil::write_atomic;
use veronica_core::packaging::Packaging;
use veronica_core::settings::Settings;
use veronica_protocol::bundled::{
    BundledIndex, BundledPackage, BUNDLED_INDEX, BUNDLED_SCHEMA, NEVER_BUNDLED,
};
use veronica_protocol::catalog::{Catalog, CatalogExtension};
use veronica_protocol::manifest::{Manifest, Profile};
use veronica_protocol::PackageVersion;

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

const MANIFEST: &str = "manifest.json";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

const LEGACY_IDS: [(&str, &str); 3] = [
    ("music", "media"),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledListing {
    pub entry: CatalogExtension,
    pub version: PackageVersion,
    pub only_bundled: bool,
}

pub fn listing(manifest: &Manifest, version: PackageVersion) -> CatalogExtension {
    CatalogExtension {
        id: manifest.id.clone(),
        title: manifest.title.clone(),
        summary: manifest.summary.clone(),
        category: manifest.category.clone(),
        icon: manifest.icon.clone(),
        labs: manifest.labs,
        profiles: manifest.profiles.clone(),
        suggest_for: manifest.suggest_for.clone(),
        permissions: manifest.permissions.clone(),
        requires: manifest.requires.clone(),
        contracts: manifest.contracts.clone(),
        latest: version,
        versions: Vec::new(),
    }
}

pub fn newest(
    published: Option<PackageVersion>,
    bundled: Option<PackageVersion>,
) -> Option<PackageVersion> {
    published.max(bundled)
}

fn packaged_manifest(bytes: &[u8]) -> Result<Manifest> {
    let decoder = zstd::stream::read::Decoder::new(bytes)?;
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries()? {
        let entry = entry?;
        if entry.path()? != Path::new(MANIFEST) {
            continue;
        }
        let mut raw = String::new();
        entry.take(MAX_MANIFEST_BYTES).read_to_string(&mut raw)?;
        return Manifest::parse(&raw).context("the bundled manifest is invalid");
    }
    bail!("the bundled package has no {MANIFEST}")
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

    pub fn manifest(&self, id: &str) -> Result<Manifest> {
        let (_, bytes) = self.verified(id)?;
        packaged_manifest(bytes.as_slice()).with_context(|| format!("cannot read {id}"))
    }

    pub fn listings(&self, catalog: &Catalog) -> Vec<BundledListing> {
        self.index
            .extensions
            .iter()
            .filter_map(|package| {
                let (entry, only_bundled) = match catalog.extension(&package.id) {
                    Some(entry) => (entry.clone(), false),
                    None => (
                        listing(&self.manifest(&package.id).ok()?, package.version),
                        true,
                    ),
                };
                Some(BundledListing {
                    entry,
                    version: package.version,
                    only_bundled,
                })
            })
            .collect()
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
        let switches: Vec<(&str, bool, &str)> = ENTRIES
            .iter()
            .map(|entry| (entry.defaults_key, entry.featured, extension_id(entry.id)))
            .chain(
                LEGACY_TOGGLES
                    .iter()
                    .map(|toggle| (toggle.defaults_key, toggle.featured, toggle.extension)),
            )
            .collect();
        let legacy_user = switches
            .iter()
            .any(|(key, _, _)| settings.get(key).is_some());
        let toggled: BTreeSet<&str> = switches
            .iter()
            .filter(|(key, featured, _)| {
                let explicit = settings.get(key) == Some(&Value::Bool(true));
                explicit || (legacy_user && settings.bool_or(key, *featured))
            })
            .map(|(_, _, id)| *id)
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
