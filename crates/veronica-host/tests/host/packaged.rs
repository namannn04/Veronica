use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context as _;
use serde_json::Value;
use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::archive::{self, FILE_INDEX};
use veronica_host::fetch::{Response, Transport};
use veronica_host::host_api::HostServices;
use veronica_host::trust::{verify_envelope, verify_with, TrustError, TrustedKey};
use veronica_host::update::install_from_catalog;
use veronica_host::{HostIdentity, Installer, Layout, Router};
use veronica_protocol::catalog::{FileIndex, PACKAGE_URL_PREFIX};
use veronica_protocol::{ExitCode, PackageVersion};

const ID: &str = "fixture";
const BROKEN_BACKEND: &[u8] = b"#!/bin/sh\nexit 3\n";

struct LocalReleases {
    dir: PathBuf,
}

impl Transport for LocalReleases {
    fn get(&self, url: &str, _etag: Option<&str>) -> anyhow::Result<Response> {
        let name = url
            .strip_prefix(PACKAGE_URL_PREFIX)
            .and_then(|rest| rest.rsplit('/').next())
            .with_context(|| format!("{url} is not a release asset"))?;
        let file = File::open(self.dir.join(name))
            .with_context(|| format!("{name} was not built into {}", self.dir.display()))?;
        Ok(Response::Body {
            etag: None,
            body: Box::new(file),
        })
    }
}

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set to run this test"))
}

fn leaked(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

fn assert_verified(tree: &Path) {
    let listing = fs::read(tree.join(FILE_INDEX)).unwrap();
    let index: FileIndex = serde_json::from_slice(&listing).unwrap();
    assert!(index.contains_key("bin/backend"));
    archive::verify_tree(tree, &index).unwrap();
}

fn echo(router: &Router, text: &str) -> Value {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let argv = ["fixture", "echo", text].map(ToString::to_string).to_vec();
    let result = runtime.block_on(router.run_cli(ID, argv, true, false));
    assert_eq!(result.exit_code, ExitCode::Success, "{result:?}");
    result.document.unwrap()
}

fn repackage(
    tree: &Path,
    work: &TempDir,
    version: PackageVersion,
    backend: Option<&[u8]>,
) -> Vec<u8> {
    let source = work.path().join(version.to_string());
    for entry in walkdir::WalkDir::new(tree).follow_links(true) {
        let entry = entry.unwrap();
        let target = source.join(entry.path().strip_prefix(tree).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).unwrap();
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
    let manifest_path = source.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["version"] = version.to_string().into();
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    if let Some(backend) = backend {
        fs::write(source.join("bin/backend"), backend).unwrap();
    }
    let mut bytes = Vec::new();
    archive::pack(&source, &mut bytes).unwrap();
    bytes
}

fn next(version: PackageVersion, step: u32) -> PackageVersion {
    PackageVersion {
        patch: version.patch + step,
        ..version
    }
}

#[test]
#[ignore = "needs packages and a catalog signed by the extensions workflow"]
fn a_built_package_installs_from_a_signed_catalog_verifies_and_rolls_back() {
    let releases = LocalReleases {
        dir: PathBuf::from(required("VERONICA_PACKAGES_DIR")),
    };
    let envelope = fs::read(required("VERONICA_CATALOG")).unwrap();
    let key = TrustedKey {
        id: leaked(required("VERONICA_CATALOG_KEY_ID")),
        public: leaked(required("VERONICA_CATALOG_PUBLIC_KEY")),
    };
    assert_eq!(
        verify_envelope(&envelope).unwrap_err(),
        TrustError::UnknownKey
    );
    let catalog = verify_with(&envelope, &[Some(key)]).unwrap();
    let published = catalog.extension(ID).unwrap().versions[0].version;

    let home = tempfile::tempdir().unwrap();
    let directories = AppDirectories::with_env(home.path(), EnvOverrides::default());
    let layout = Layout::new(&directories);
    let services = Arc::new(HostServices::new(
        layout.clone(),
        directories.settings_file(),
    ));
    let installer = Installer::new(layout.clone(), HostIdentity::current())
        .with_health_check(Arc::clone(&services));
    let router = Router::oneshot(Arc::clone(&services));
    let current = layout.current_link(ID);

    let installed = install_from_catalog(&installer, &catalog, &releases, ID).unwrap();
    assert_eq!(installed.version, published);
    assert_verified(&current);
    let document = echo(&router, "packaged");
    assert_eq!(document["text"], "packaged");
    assert_eq!(document["mode"], "oneshot");

    let work = tempfile::tempdir().unwrap();
    let upgrade = next(published, 1);
    let bytes = repackage(&current, &work, upgrade, None);
    let upgraded = installer
        .install_archive(bytes.as_slice(), ID, upgrade)
        .unwrap();
    assert_eq!(
        (upgraded.version, upgraded.previous),
        (upgrade, Some(published))
    );
    assert_verified(&current);

    let broken = next(published, 2);
    let bytes = repackage(&current, &work, broken, Some(BROKEN_BACKEND));
    let error = installer
        .install_archive(bytes.as_slice(), ID, broken)
        .unwrap_err();
    assert!(error.to_string().contains("health check"), "{error:#}");
    let registry = installer.registry().unwrap();
    let entry = registry.get(ID).unwrap();
    assert_eq!(entry.version, upgrade);
    assert!(entry.quarantined.contains(&broken));
    assert!(!layout.version_dir(ID, broken).exists());

    let restored = installer.rollback(ID).unwrap();
    assert_eq!(
        (restored.version, restored.previous),
        (published, Some(upgrade))
    );
    assert_eq!(
        fs::read_link(&current).unwrap(),
        Path::new(&published.to_string())
    );
    assert_verified(&current);
    assert_eq!(echo(&router, "restored")["text"], "restored");
}
