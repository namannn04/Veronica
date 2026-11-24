use std::fs::{self, File};
use std::io::{self, Read};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, ensure, Context, Result};
use chrono::{SecondsFormat, Utc};
use tempfile::TempDir;
use veronica_protocol::canonical;
use veronica_protocol::manifest::{Engines, Target};
use veronica_protocol::{Manifest, PackageVersion};

use crate::archive::{self, Limits, FILE_INDEX};
use crate::connection::BackendError;
use crate::host_api::HostServices;
use crate::layout::Layout;
use crate::lock::ExtensionsLock;
use crate::registry::{InstalledExtension, Origin, Registry};
use crate::supervisor::{verify_starts, CrashRecord};

pub const HOST_API: u32 = 1;
const TREE: &str = "tree";
const CURRENT: &str = "current";
const PREVIOUS: &str = "previous";

#[cfg(target_arch = "aarch64")]
pub const HOST_TARGET: Target = Target::LinuxAarch64;
#[cfg(not(target_arch = "aarch64"))]
pub const HOST_TARGET: Target = Target::LinuxX86_64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostIdentity {
    pub version: semver::Version,
    pub api: u32,
    pub target: Target,
}

impl HostIdentity {
    pub fn current() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION")
                .parse()
                .expect("the crate version is valid semver"),
            api: HOST_API,
            target: HOST_TARGET,
        }
    }

    pub fn check_engines(&self, id: &str, engines: &Engines) -> Result<()> {
        let range = semver::VersionReq::parse(&engines.host)
            .with_context(|| format!("engines.host {:?} is not a version range", engines.host))?;
        ensure!(
            range.matches(&self.version),
            "{id} needs host {range}, this host is {}",
            self.version
        );
        ensure!(
            engines.host_api <= self.api,
            "{id} needs host API {}, this host provides {}",
            engines.host_api,
            self.api
        );
        Ok(())
    }

    pub fn check(&self, manifest: &Manifest) -> Result<()> {
        self.check_engines(&manifest.id, &manifest.engines)?;
        ensure!(
            manifest.targets.contains(&self.target),
            "{} is not built for {}",
            manifest.id,
            self.target.as_str()
        );
        Ok(())
    }
}

#[derive(Clone)]
pub struct Installer {
    layout: Layout,
    host: HostIdentity,
    limits: Limits,
    health_check: Option<Arc<HostServices>>,
}

fn check_backend(services: &Arc<HostServices>, id: &str) -> Result<(), BackendError> {
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| BackendError::Launch(error.to_string()))?
                    .block_on(verify_starts(services, id))
            })
            .join()
            .unwrap_or_else(|_| Err(BackendError::Launch("the health check panicked".into())))
    })
}

fn read_manifest(root: &Path) -> Result<Manifest> {
    let path = root.join("manifest.json");
    let raw =
        fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
    Manifest::parse(&raw).with_context(|| format!("{} is invalid", path.display()))
}

fn sync_tree(root: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(root).contents_first(true) {
        let entry = entry?;
        if entry.file_type().is_dir() {
            File::open(entry.path())?.sync_all()?;
        }
    }
    Ok(())
}

fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

fn remove_if_present(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

struct Placement {
    target: PathBuf,
    displaced: Option<PathBuf>,
}

impl Placement {
    fn undo(self) {
        let _ = fs::remove_dir_all(&self.target);
        if let Some(displaced) = self.displaced {
            let _ = fs::rename(displaced, &self.target);
        }
    }
}

impl Installer {
    pub fn new(layout: Layout, host: HostIdentity) -> Self {
        Self {
            layout,
            host,
            limits: Limits::default(),
            health_check: None,
        }
    }

    #[must_use]
    pub fn with_health_check(mut self, services: Arc<HostServices>) -> Self {
        self.health_check = Some(services);
        self
    }

    #[must_use]
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn host(&self) -> &HostIdentity {
        &self.host
    }

    pub fn registry(&self) -> Result<Registry> {
        Registry::load(&self.layout.installed_file())
    }

    fn lock(&self) -> Result<ExtensionsLock> {
        let lock = ExtensionsLock::acquire(&self.layout.lock_file())?;
        let staging = self.layout.staging_dir();
        remove_if_present(fs::remove_dir_all(&staging))
            .with_context(|| format!("cannot clear {}", staging.display()))?;
        fs::create_dir_all(&staging)
            .with_context(|| format!("cannot create {}", staging.display()))?;
        Ok(lock)
    }

    fn stage(&self) -> Result<TempDir> {
        let staged = tempfile::Builder::new()
            .prefix("stage-")
            .tempdir_in(self.layout.staging_dir())?;
        fs::create_dir(staged.path().join(TREE))?;
        Ok(staged)
    }

    pub fn install_archive(
        &self,
        reader: impl Read,
        id: &str,
        version: PackageVersion,
    ) -> Result<InstalledExtension> {
        let _lock = self.lock()?;
        let staged = self.stage()?;
        archive::extract(reader, &staged.path().join(TREE), &self.limits)?;
        self.activate(&staged, id, version, Origin::Catalog)
    }

    pub fn install_sideload(&self, source: &Path) -> Result<InstalledExtension> {
        let _lock = self.lock()?;
        let index = archive::index_tree(source, &self.limits)?;
        let staged = self.stage()?;
        let tree = staged.path().join(TREE);
        for (path, entry) in &index {
            let file = File::open(source.join(path))?.take(entry.size);
            archive::write_file(&tree, Path::new(path), entry.mode, file)?;
        }
        let listing = canonical::to_string_pretty(&index)?;
        archive::write_file(&tree, Path::new(FILE_INDEX), 0o644, listing.as_bytes())?;
        archive::verify_tree(&tree, &index)?;
        let manifest = read_manifest(&tree)?;
        self.activate(
            &staged,
            &manifest.id,
            manifest.version.with_patch(0),
            Origin::Sideload,
        )
    }

    fn activate(
        &self,
        staged: &TempDir,
        id: &str,
        version: PackageVersion,
        origin: Origin,
    ) -> Result<InstalledExtension> {
        let tree = staged.path().join(TREE);
        let manifest = read_manifest(&tree)?;
        ensure!(
            manifest.id == id,
            "package is {:?}, expected {id:?}",
            manifest.id
        );
        let expected = match origin {
            Origin::Sideload => manifest.version.authored() == version.authored(),
            Origin::Builtin | Origin::Catalog => manifest.version.stamped() == Some(version),
        };
        ensure!(
            expected,
            "package {id} is version {}, expected {version}",
            manifest.version
        );
        self.host.check(&manifest)?;
        sync_tree(&tree)?;

        let mut registry = self.registry()?;
        let before = registry.get(id).cloned();
        let placement = self.place(staged, &tree, id, version)?;
        let entry = InstalledExtension {
            version,
            origin,
            enabled: before.as_ref().is_none_or(|entry| entry.enabled),
            installed_at: now(),
            previous: match &before {
                Some(entry) if entry.version != version => Some(entry.version),
                Some(entry) => entry.previous,
                None => None,
            },
            quarantined: before
                .as_ref()
                .map(|entry| entry.quarantined.clone())
                .unwrap_or_default(),
        };
        if let Err(error) = self.switch(&mut registry, id, entry.clone(), before.as_ref()) {
            placement.undo();
            return Err(error);
        }
        if let (Some(services), Some(_)) = (&self.health_check, &manifest.backend) {
            if let Err(error) = check_backend(services, id) {
                self.reject(&mut registry, id, &entry, before)?;
                placement.undo();
                bail!("{id} {version} failed its health check and was rolled back: {error}");
            }
        }
        CrashRecord::clear(&self.layout, id)?;
        if let Some(displaced) = placement.displaced {
            let _ = fs::remove_dir_all(displaced);
        }
        self.prune(id, &entry)?;
        Ok(entry)
    }

    fn place(
        &self,
        staged: &TempDir,
        tree: &Path,
        id: &str,
        version: PackageVersion,
    ) -> Result<Placement> {
        let package = self.layout.package_dir(id);
        fs::create_dir_all(&package)?;
        let target = self.layout.version_dir(id, version);
        let displaced = if target.exists() {
            let aside = staged.path().join("displaced");
            fs::rename(&target, &aside)?;
            Some(aside)
        } else {
            None
        };
        if let Err(error) = fs::rename(tree, &target) {
            if let Some(aside) = &displaced {
                let _ = fs::rename(aside, &target);
            }
            return Err(error)
                .with_context(|| format!("cannot move package into {}", target.display()));
        }
        sync_dir(&package)?;
        Ok(Placement { target, displaced })
    }

    fn point(&self, id: &str, name: &str, version: Option<PackageVersion>) -> Result<()> {
        let package = self.layout.package_dir(id);
        let link = package.join(name);
        let Some(version) = version else {
            return Ok(remove_if_present(fs::remove_file(&link))?);
        };
        let temporary = package.join(format!(".{name}.tmp"));
        remove_if_present(fs::remove_file(&temporary))?;
        symlink(version.to_string(), &temporary)?;
        fs::rename(&temporary, &link).with_context(|| format!("cannot swap {}", link.display()))?;
        sync_dir(&package)
    }

    fn switch(
        &self,
        registry: &mut Registry,
        id: &str,
        entry: InstalledExtension,
        before: Option<&InstalledExtension>,
    ) -> Result<()> {
        let applied = (|| -> Result<()> {
            self.point(id, PREVIOUS, entry.previous)?;
            self.point(id, CURRENT, Some(entry.version))?;
            registry.extensions.insert(id.to_string(), entry);
            registry.save(&self.layout.installed_file())
        })();
        if applied.is_err() {
            let _ = self.point(id, CURRENT, before.map(|entry| entry.version));
            let _ = self.point(id, PREVIOUS, before.and_then(|entry| entry.previous));
        }
        applied
    }

    fn prune(&self, id: &str, entry: &InstalledExtension) -> Result<()> {
        let keep = [Some(entry.version), entry.previous]
            .into_iter()
            .flatten()
            .map(|version| version.to_string())
            .collect::<Vec<_>>();
        for child in fs::read_dir(self.layout.package_dir(id))? {
            let child = child?;
            let name = child.file_name().to_string_lossy().into_owned();
            if name == CURRENT || name == PREVIOUS || keep.contains(&name) {
                continue;
            }
            if child.file_type()?.is_dir() {
                fs::remove_dir_all(child.path())?;
            } else {
                fs::remove_file(child.path())?;
            }
        }
        Ok(())
    }

    fn reject(
        &self,
        registry: &mut Registry,
        id: &str,
        entry: &InstalledExtension,
        before: Option<InstalledExtension>,
    ) -> Result<()> {
        let Some(mut restored) = before else {
            registry.extensions.remove(id);
            registry.save(&self.layout.installed_file())?;
            self.point(id, CURRENT, None)?;
            return self.point(id, PREVIOUS, None);
        };
        restored.quarantined.insert(entry.version);
        self.switch(registry, id, restored, Some(entry))
    }

    pub fn rollback(&self, id: &str) -> Result<InstalledExtension> {
        let _lock = self.lock()?;
        let mut registry = self.registry()?;
        let Some(before) = registry.get(id).cloned() else {
            bail!("{id} is not installed");
        };
        let Some(previous) = before.previous else {
            bail!("{id} has no previous version to roll back to");
        };
        ensure!(
            self.layout.version_dir(id, previous).is_dir(),
            "{id} {previous} is no longer on disk"
        );
        let entry = InstalledExtension {
            version: previous,
            previous: Some(before.version),
            installed_at: now(),
            ..before.clone()
        };
        self.switch(&mut registry, id, entry.clone(), Some(&before))?;
        Ok(entry)
    }

    pub fn remove(&self, id: &str, purge: bool) -> Result<()> {
        let _lock = self.lock()?;
        let mut registry = self.registry()?;
        ensure!(
            registry.extensions.remove(id).is_some(),
            "{id} is not installed"
        );
        registry.save(&self.layout.installed_file())?;
        remove_if_present(fs::remove_dir_all(self.layout.package_dir(id)))?;
        remove_if_present(fs::remove_file(self.layout.dev_link(id)))?;
        if purge {
            remove_if_present(fs::remove_dir_all(self.layout.data_dir(id)))?;
        }
        Ok(())
    }
}
