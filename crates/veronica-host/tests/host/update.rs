use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde_json::Value;
use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::catalog::CatalogStore;
use veronica_host::fetch::{download, FetchError};
use veronica_host::registry::{InstalledExtension, Origin, Registry};
use veronica_host::update::{
    install_from_catalog, plan, resolve, AutoUpdate, Decision, Skip, Trigger,
};
use veronica_host::{HostIdentity, Installer, Layout};
use veronica_pack::fingerprint::Stamp;
use veronica_pack::pack::{pack, PackInputs, Packed};
use veronica_protocol::catalog::{Catalog, TargetArtifact, CATALOG_URL};
use veronica_protocol::manifest::Target;
use veronica_protocol::PackageVersion;

use crate::common::{
    catalog_of, manifest, metadata, now, signed, write_tree, FakeTransport, REVISION,
};

fn host() -> HostIdentity {
    HostIdentity {
        version: semver::Version::new(0, 5, 0),
        api: 1,
        target: Target::LinuxX86_64,
    }
}

fn installer() -> (TempDir, Installer) {
    let home = tempfile::tempdir().unwrap();
    let layout = Layout::new(&AppDirectories::with_env(
        home.path(),
        EnvOverrides::default(),
    ));
    (home, Installer::new(layout, host()))
}

fn v(raw: &str) -> PackageVersion {
    raw.parse().unwrap()
}

fn extension_repo() -> TempDir {
    let repo = tempfile::tempdir().unwrap();
    let mut authored: Value = serde_json::from_slice(&manifest("probe", "1.0")).unwrap();
    authored["engines"]["backendProtocol"] = 1.into();
    authored["backend"] = serde_json::json!({ "entry": "bin/backend" });
    let authored = serde_json::to_vec_pretty(&authored).unwrap();
    write_tree(
        repo.path(),
        &[
            ("extensions/probe/manifest.json", &authored, 0o644),
            ("LICENSE", b"license text\n", 0o644),
            ("NOTICE", b"notice text\n", 0o644),
        ],
    );
    repo
}

fn pack_release(repo: &Path, version: &str) -> (Packed, Vec<u8>) {
    let backend = repo.join(format!("build/backend-{version}"));
    write_tree(
        repo,
        &[(
            &format!("build/backend-{version}"),
            version.as_bytes(),
            0o755,
        )],
    );
    let stamp = Stamp {
        id: "probe".to_string(),
        version: v(version),
        fingerprint: format!("sha256:{version}"),
        commit: "0123abcd".to_string(),
        published_at: "2026-11-20T08:30:00Z".to_string(),
        inputs: Vec::new(),
    };
    let packed = pack(
        &PackInputs {
            root: repo,
            backend: Some(&backend),
            ui: None,
            out: &repo.join("out"),
            target: Target::LinuxX86_64,
        },
        &stamp,
    )
    .unwrap();
    let bytes = fs::read(&packed.archive).unwrap();
    (packed, bytes)
}

fn publish(transport: &FakeTransport, releases: &[(Packed, Vec<u8>)], revision: u64) -> Catalog {
    for (packed, bytes) in releases {
        let artifact = artifact_url(packed);
        transport.serve(&artifact, bytes.clone(), None);
    }
    let catalog = catalog_of(
        releases
            .iter()
            .map(|(packed, _)| packed.metadata.clone())
            .collect(),
        revision,
    );
    transport.serve(CATALOG_URL, signed(&catalog), None);
    catalog
}

fn artifact_url(packed: &Packed) -> String {
    veronica_protocol::catalog::package_url("probe", packed.metadata.version, Target::LinuxX86_64)
}

#[test]
fn a_catalog_release_installs_and_updates_end_to_end() {
    let (_home, installer) = installer();
    let store = CatalogStore::new(installer.layout().clone());
    let transport = FakeTransport::default();
    let repo = extension_repo();
    let first = pack_release(repo.path(), "1.0.3");
    publish(&transport, std::slice::from_ref(&first), REVISION);

    let catalog = store.refresh(&transport, now()).unwrap().catalog;
    let entry = install_from_catalog(&installer, &catalog, &transport, "probe").unwrap();
    assert_eq!((entry.version, entry.origin), (v("1.0.3"), Origin::Catalog));
    let tree = installer.layout().current_link("probe");
    assert_eq!(fs::read(tree.join("bin/backend")).unwrap(), b"1.0.3");
    let stamped: Value =
        serde_json::from_slice(&fs::read(tree.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(stamped["version"], "1.0.3");
    assert!(fs::read_dir(installer.layout().downloads_dir())
        .unwrap()
        .next()
        .is_none());

    let second = pack_release(repo.path(), "1.0.4");
    publish(&transport, &[first, second], REVISION + 1);
    let catalog = store.refresh(&transport, now()).unwrap().catalog;
    let decisions = plan(
        &installer.registry().unwrap(),
        &catalog,
        installer.host(),
        &AutoUpdate::default(),
        Trigger::Automatic,
    );
    assert_eq!(
        decisions["probe"],
        Decision::Update {
            from: v("1.0.3"),
            to: v("1.0.4")
        }
    );
    let entry = install_from_catalog(&installer, &catalog, &transport, "probe").unwrap();
    assert_eq!(
        (entry.version, entry.previous),
        (v("1.0.4"), Some(v("1.0.3")))
    );
    assert_eq!(fs::read(tree.join("bin/backend")).unwrap(), b"1.0.4");

    assert!(install_from_catalog(&installer, &catalog, &transport, "absent").is_err());
}

#[test]
fn a_package_whose_manifest_disagrees_with_the_catalog_is_refused() {
    let (_home, installer) = installer();
    let transport = FakeTransport::default();
    let repo = extension_repo();
    let (packed, bytes) = pack_release(repo.path(), "1.0.3");
    let mut mislabelled = packed.metadata;
    mislabelled.version = v("1.0.5");
    let catalog = catalog_of(vec![mislabelled], REVISION);
    let url = veronica_protocol::catalog::package_url("probe", v("1.0.5"), Target::LinuxX86_64);
    transport.serve(&url, bytes, None);
    let error = install_from_catalog(&installer, &catalog, &transport, "probe").unwrap_err();
    assert!(error.to_string().contains("expected 1.0.5"), "{error:#}");
    assert!(installer.registry().unwrap().extensions.is_empty());
}

fn artifact(url: &str, body: &[u8]) -> TargetArtifact {
    TargetArtifact {
        url: url.to_string(),
        size: body.len() as u64,
        sha256: veronica_pack::sha256_hex(body),
        unpacked_size: 0,
    }
}

fn fetch_error(error: &anyhow::Error) -> FetchError {
    error.downcast_ref::<FetchError>().unwrap().clone()
}

#[test]
fn downloads_are_checked_against_the_catalog() {
    let (_home, installer) = installer();
    let layout = installer.layout();
    let transport = FakeTransport::default();
    let url = veronica_protocol::catalog::package_url("probe", v("1.0.1"), Target::LinuxX86_64);
    let body = b"package bytes".to_vec();
    transport.serve(&url, body.clone(), None);
    let good = artifact(&url, &body);

    let fetched = download(layout, &transport, &good).unwrap();
    assert_eq!(fetched.path(), layout.download_part(&good.sha256));
    assert_eq!(fs::read(fetched.path()).unwrap(), body);
    drop(fetched);
    assert!(!layout.download_part(&good.sha256).exists());

    let mut wrong_hash = good.clone();
    wrong_hash.sha256 = "0".repeat(64);
    let error = download(layout, &transport, &wrong_hash).unwrap_err();
    assert!(matches!(
        fetch_error(&error),
        FetchError::HashMismatch { .. }
    ));

    let mut longer = good.clone();
    longer.size += 1;
    assert_eq!(
        fetch_error(&download(layout, &transport, &longer).unwrap_err()),
        FetchError::SizeMismatch {
            expected: longer.size,
            actual: good.size
        }
    );

    let mut shorter = good.clone();
    shorter.size -= 1;
    assert_eq!(
        fetch_error(&download(layout, &transport, &shorter).unwrap_err()),
        FetchError::TooLarge {
            expected: shorter.size
        }
    );

    let mut bad_digest = good;
    bad_digest.sha256 = "../../escape".to_string();
    assert!(matches!(
        fetch_error(&download(layout, &transport, &bad_digest).unwrap_err()),
        FetchError::BadDigest(_)
    ));

    for outside in [
        "https://example.org/veronica-ext-probe-1.0.1-linux-x86_64.tar.zst",
        "https://github.com/namannn04/veronica/releases/download/v1.0.0/probe.tar.zst",
    ] {
        transport.serve(outside, body.clone(), None);
        let error = download(layout, &transport, &artifact(outside, &body)).unwrap_err();
        assert!(matches!(fetch_error(&error), FetchError::UrlNotAllowed(_)));
    }
    let requested: BTreeSet<_> = transport
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|(url, _)| url.clone())
        .collect();
    assert_eq!(requested, BTreeSet::from([url]));
    assert!(fs::read_dir(layout.downloads_dir())
        .unwrap()
        .next()
        .is_none());
}

fn installed(version: &str, origin: Origin, quarantined: &[&str]) -> InstalledExtension {
    InstalledExtension {
        version: v(version),
        origin,
        enabled: true,
        installed_at: "2026-11-20T00:00:00Z".to_string(),
        previous: None,
        quarantined: quarantined.iter().map(|raw| v(raw)).collect(),
    }
}

fn matrix_catalog() -> Catalog {
    let sha = "a".repeat(64);
    let mut releases = Vec::new();
    for (id, versions) in [
        ("fresh", &["1.0.1", "1.0.2"][..]),
        ("paused", &["1.0.1", "1.0.2"]),
        ("dev", &["1.0.1", "1.0.2"]),
        ("builtin", &["1.0.1", "1.0.2"]),
        ("yanked", &["1.0.1", "1.0.2"]),
        ("quarantined", &["1.0.1", "1.0.2", "1.0.3"]),
        ("newer-host", &["1.0.1", "1.0.2"]),
        ("arm-only", &["1.0.2"]),
        ("current", &["1.0.2"]),
    ] {
        for version in versions {
            releases.push(metadata(id, version, &sha, 10));
        }
    }
    let mut catalog = catalog_of(releases, REVISION);
    for extension in &mut catalog.extensions {
        for entry in &mut extension.versions {
            match (extension.id.as_str(), entry.version.to_string().as_str()) {
                ("yanked", "1.0.2") => entry.yanked = true,
                ("newer-host", "1.0.2") => entry.engines.host = ">=9.0.0".to_string(),
                ("arm-only", _) => {
                    let artifact = entry.targets.remove(&Target::LinuxX86_64).unwrap();
                    entry.targets.insert(Target::LinuxAarch64, artifact);
                }
                _ => {}
            }
        }
    }
    catalog
}

fn matrix_registry() -> Registry {
    let mut registry = Registry::default();
    for (id, entry) in [
        ("fresh", installed("1.0.1", Origin::Catalog, &[])),
        ("paused", installed("1.0.1", Origin::Catalog, &[])),
        ("dev", installed("1.0.1", Origin::Sideload, &[])),
        ("builtin", installed("1.0.1", Origin::Builtin, &[])),
        ("yanked", installed("1.0.1", Origin::Catalog, &[])),
        (
            "quarantined",
            installed("1.0.1", Origin::Catalog, &["1.0.3"]),
        ),
        ("newer-host", installed("1.0.1", Origin::Catalog, &[])),
        ("arm-only", installed("1.0.1", Origin::Catalog, &[])),
        ("current", installed("1.0.2", Origin::Catalog, &[])),
        ("retired", installed("1.0.1", Origin::Catalog, &[])),
    ] {
        registry.extensions.insert(id.to_string(), entry);
    }
    registry
}

#[test]
fn the_update_planner_follows_origin_status_and_settings() {
    let catalog = matrix_catalog();
    let registry = matrix_registry();
    let settings = AutoUpdate {
        disabled: BTreeSet::from(["paused".to_string()]),
        ..AutoUpdate::default()
    };
    let update = |from: &str, to: &str| Decision::Update {
        from: v(from),
        to: v(to),
    };
    let automatic = plan(&registry, &catalog, &host(), &settings, Trigger::Automatic);
    let expected: BTreeMap<String, Decision> = [
        ("fresh", update("1.0.1", "1.0.2")),
        ("paused", Decision::Skip(Skip::AutoUpdateOff)),
        ("dev", Decision::Skip(Skip::NotFromCatalog)),
        ("builtin", Decision::Skip(Skip::NotFromCatalog)),
        ("yanked", Decision::Skip(Skip::UpToDate)),
        ("quarantined", update("1.0.1", "1.0.2")),
        ("newer-host", Decision::Skip(Skip::UpToDate)),
        ("arm-only", Decision::Skip(Skip::NoCompatibleVersion)),
        ("current", Decision::Skip(Skip::UpToDate)),
        ("retired", Decision::Skip(Skip::NotInCatalog)),
    ]
    .into_iter()
    .map(|(id, decision)| (id.to_string(), decision))
    .collect();
    assert_eq!(automatic, expected);

    let manual = plan(&registry, &catalog, &host(), &settings, Trigger::Manual);
    assert_eq!(manual["paused"], update("1.0.1", "1.0.2"));

    let off = AutoUpdate {
        enabled: false,
        ..AutoUpdate::default()
    };
    let all_off = plan(&registry, &catalog, &host(), &off, Trigger::Automatic);
    assert!(all_off
        .values()
        .all(|decision| matches!(decision, Decision::Skip(_))));
    assert_eq!(all_off["fresh"], Decision::Skip(Skip::AutoUpdateOff));
    assert_eq!(all_off["dev"], Decision::Skip(Skip::NotFromCatalog));

    let none = BTreeSet::new();
    let (entry, _) = resolve(&catalog, "quarantined", &host(), &none).unwrap();
    assert_eq!(entry.version, v("1.0.3"));
    assert!(resolve(&catalog, "arm-only", &host(), &none).is_none());
}
