use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use veronica_core::settings::Settings;
use veronica_host::bundled::{bundled_dir, claim_due, install_preferred, newest, Bundled};
use veronica_host::catalog::CatalogStore;
use veronica_host::launcher::LaunchEnvironment;
use veronica_host::registry::Origin;
use veronica_host::update::{plan, AutoUpdate, Decision, Trigger};
use veronica_pack::bundle::bundle;
use veronica_pack::pack::Packed;
use veronica_protocol::bundled::BUNDLED_INDEX;
use veronica_protocol::catalog::Catalog;

use crate::common::{now, FakeTransport, REVISION};
use crate::update::{extension_repo, installer, pack_release, publish, v};

fn bundled_from(root: &Path, release: &Packed) -> Bundled {
    let packages = root.join("packages");
    fs::create_dir_all(&packages).unwrap();
    let name = release.archive.file_name().unwrap().to_str().unwrap();
    fs::copy(&release.archive, packages.join(name)).unwrap();
    fs::copy(
        release.archive.with_file_name(format!("{name}.json")),
        packages.join(format!("{name}.json")),
    )
    .unwrap();
    let dir = root.join("bundled");
    bundle(&packages, &dir).unwrap();
    fs::remove_dir_all(&packages).unwrap();
    Bundled::open(&dir).unwrap().unwrap()
}

fn environment(packaging: &[(&str, &str)], sandboxed: bool) -> LaunchEnvironment {
    LaunchEnvironment::from_env(sandboxed, None, |name| {
        packaging
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| OsString::from(value))
    })
}

#[test]
fn every_packaging_has_its_bundled_directory() {
    let dir = |vars: &[(&str, &str)], sandboxed| bundled_dir(&environment(vars, sandboxed));
    assert_eq!(
        dir(&[], false),
        Some(PathBuf::from("/usr/share/veronica/extensions"))
    );
    assert_eq!(
        dir(&[], true),
        Some(PathBuf::from("/app/share/veronica/extensions"))
    );
    assert_eq!(
        dir(
            &[("SNAP_NAME", "veronica"), ("SNAP", "/snap/veronica/42")],
            false
        ),
        Some(PathBuf::from(
            "/snap/veronica/42/usr/share/veronica/extensions"
        ))
    );
    assert_eq!(
        dir(
            &[
                ("APPIMAGE", "/home/me/Veronica.AppImage"),
                ("APPDIR", "/tmp/.mount_Veron")
            ],
            false
        ),
        Some(PathBuf::from(
            "/tmp/.mount_Veron/usr/share/veronica/extensions"
        ))
    );
    assert_eq!(dir(&[("APPIMAGE", "/x.AppImage")], false), None);
}

#[test]
fn an_empty_or_missing_bundle_offers_nothing() {
    let root = tempfile::tempdir().unwrap();
    assert!(Bundled::open(root.path()).unwrap().is_none());
    bundle(&root.path().join("none"), root.path()).unwrap();
    let bundled = Bundled::open(root.path()).unwrap().unwrap();
    assert!(bundled.index().extensions.is_empty());
    let mut settings = Settings::default();
    settings.set("profile", json!("developer"));
    assert!(bundled.wanted(&settings).is_empty());
}

#[test]
fn indexes_naming_the_test_fixture_or_another_schema_are_refused() {
    let root = tempfile::tempdir().unwrap();
    let index = |value: Value| fs::write(root.path().join(BUNDLED_INDEX), value.to_string());
    index(json!({ "schema": 1, "extensions": [
        { "id": "fixture", "version": "0.1.1", "sha256": "00", "size": 1 }
    ] }))
    .unwrap();
    assert!(Bundled::open(root.path()).is_err());
    index(json!({ "schema": 2, "extensions": [] })).unwrap();
    assert!(Bundled::open(root.path()).is_err());
}

#[test]
fn a_bundled_package_is_verified_and_installed_with_the_bundled_origin() {
    let root = tempfile::tempdir().unwrap();
    let repo = extension_repo();
    let (release, _) = pack_release(repo.path(), "1.0.3");
    let bundled = bundled_from(root.path(), &release);
    let (_home, installer) = installer();

    let entry = bundled.install(&installer, "probe").unwrap();
    assert_eq!((entry.version, entry.origin), (v("1.0.3"), Origin::Bundled));
    let tree = installer.layout().current_link("probe");
    assert_eq!(fs::read(tree.join("bin/backend")).unwrap(), b"1.0.3");

    let archive = bundled.dir().join("probe.tar.zst");
    let mut bytes = fs::read(&archive).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&archive, bytes).unwrap();
    let error = bundled.install(&installer, "probe").unwrap_err();
    assert!(error.to_string().contains("does not match"), "{error:#}");
    assert!(bundled.install(&installer, "absent").is_err());
}

#[test]
fn profiles_and_legacy_toggles_decide_what_is_installed_once() {
    let root = tempfile::tempdir().unwrap();
    let repo = extension_repo();
    let (release, _) = pack_release(repo.path(), "1.0.3");
    let bundled = bundled_from(root.path(), &release);
    let (_home, installer) = installer();

    let mut settings = Settings::default();
    assert!(claim_due(&installer, &bundled, &settings)
        .unwrap()
        .is_empty());
    settings.set("profile", json!("developer"));
    assert!(claim_due(&installer, &bundled, &settings)
        .unwrap()
        .is_empty());
    settings.set("profile", json!("simple"));
    assert_eq!(
        claim_due(&installer, &bundled, &settings).unwrap(),
        ["probe"]
    );
    assert!(claim_due(&installer, &bundled, &settings)
        .unwrap()
        .is_empty());

    bundled.install(&installer, "probe").unwrap();
    installer.remove("probe", false).unwrap();
    assert!(claim_due(&installer, &bundled, &settings)
        .unwrap()
        .is_empty());
}

#[test]
fn legacy_toggles_map_to_their_extension_ids() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join(BUNDLED_INDEX),
        json!({ "schema": 1, "extensions": [
            { "id": "color", "version": "1.0.1", "sha256": "00", "size": 1 },
            { "id": "media", "version": "1.0.1", "sha256": "00", "size": 1 },
            { "id": "clipboard", "version": "1.0.1", "sha256": "00", "size": 1 },
            { "id": "audit", "version": "1.0.1", "sha256": "00", "size": 1 }
        ] })
        .to_string(),
    )
    .unwrap();
    let bundled = Bundled::open(root.path()).unwrap().unwrap();
    let mut settings = Settings::default();
    assert!(bundled.wanted(&settings).is_empty());
    settings.set("colorPickerEnabled", json!(true));
    settings.set("clipboardEnabled", json!(false));
    assert_eq!(
        bundled.wanted(&settings),
        BTreeSet::from(["color".to_string()])
    );
    settings.set("tabSEOAuditEnabled", json!(true));
    assert_eq!(
        bundled.wanted(&settings),
        BTreeSet::from(["audit".to_string(), "color".to_string()])
    );
}

#[test]
fn a_newer_bundle_upgrades_a_bundled_install() {
    let root = tempfile::tempdir().unwrap();
    let repo = extension_repo();
    let (first, _) = pack_release(repo.path(), "1.0.3");
    let (second, _) = pack_release(repo.path(), "1.0.4");
    let (_home, installer) = installer();
    bundled_from(&root.path().join("old"), &first)
        .install(&installer, "probe")
        .unwrap();
    let newer = bundled_from(&root.path().join("new"), &second);
    let settings = Settings::default();
    assert_eq!(claim_due(&installer, &newer, &settings).unwrap(), ["probe"]);
    let entry = newer.install(&installer, "probe").unwrap();
    assert_eq!(
        (entry.version, entry.previous, entry.origin),
        (v("1.0.4"), Some(v("1.0.3")), Origin::Bundled)
    );
}

fn live_catalog(transport: &FakeTransport, releases: &[(Packed, Vec<u8>)]) -> Catalog {
    publish(transport, releases, REVISION);
    let (_home, installer) = installer();
    CatalogStore::new(installer.layout().clone())
        .refresh(transport, now())
        .unwrap()
        .catalog
}

#[test]
fn the_catalog_replaces_a_bundled_install_once_it_has_a_newer_version() {
    let root = tempfile::tempdir().unwrap();
    let repo = extension_repo();
    let first = pack_release(repo.path(), "1.0.3");
    let second = pack_release(repo.path(), "1.0.4");
    let bundled = bundled_from(root.path(), &first.0);
    let (_home, installer) = installer();
    let transport = FakeTransport::default();

    let offline = Catalog {
        extensions: Vec::new(),
        ..live_catalog(&transport, std::slice::from_ref(&first))
    };
    let entry =
        install_preferred(&installer, &offline, Some(&bundled), &transport, "probe").unwrap();
    assert_eq!((entry.version, entry.origin), (v("1.0.3"), Origin::Bundled));

    let catalog = live_catalog(&transport, &[first, second]);
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
    let entry =
        install_preferred(&installer, &catalog, Some(&bundled), &transport, "probe").unwrap();
    assert_eq!((entry.version, entry.origin), (v("1.0.4"), Origin::Catalog));
}

#[test]
fn an_unreachable_catalog_falls_back_to_the_bundled_copy() {
    let root = tempfile::tempdir().unwrap();
    let repo = extension_repo();
    let first = pack_release(repo.path(), "1.0.3");
    let second = pack_release(repo.path(), "1.0.4");
    let bundled = bundled_from(root.path(), &first.0);
    let (_home, installer) = installer();
    let catalog = live_catalog(&FakeTransport::default(), &[first, second]);

    let entry = install_preferred(
        &installer,
        &catalog,
        Some(&bundled),
        &FakeTransport::default(),
        "probe",
    )
    .unwrap();
    assert_eq!((entry.version, entry.origin), (v("1.0.3"), Origin::Bundled));
    assert!(install_preferred(
        &installer,
        &catalog,
        None,
        &FakeTransport::default(),
        "probe"
    )
    .is_err());
}

#[test]
fn the_packaging_installs_the_bundle_where_the_host_looks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config: Value = serde_json::from_str(
        &fs::read_to_string(root.join("apps/desktop/src-tauri/tauri.conf.json")).unwrap(),
    )
    .unwrap();
    for bundle in ["deb", "appimage"] {
        assert!(config["bundle"]["linux"][bundle]["files"]
            .get(veronica_host::bundled::DEB_DIR)
            .is_some());
    }
    let flatpak =
        fs::read_to_string(root.join("packaging/flatpak/io.github.namannn04.Veronica.yml"))
            .unwrap();
    assert!(flatpak.contains(&format!(
        "cp -r {} {}",
        veronica_host::bundled::PACKAGED_DIR,
        veronica_host::bundled::FLATPAK_DIR
    )));
    assert_eq!(
        format!("/{}", veronica_host::bundled::PACKAGED_DIR),
        veronica_host::bundled::DEB_DIR
    );
}

#[test]
fn bundled_only_packages_are_listed_from_their_manifest_and_catalog_entries_win() {
    let root = tempfile::tempdir().unwrap();
    let repo = extension_repo();
    let first = pack_release(repo.path(), "1.0.3");
    let second = pack_release(repo.path(), "1.0.4");
    let bundled = bundled_from(root.path(), &first.0);
    let transport = FakeTransport::default();

    let offline = Catalog {
        extensions: Vec::new(),
        ..live_catalog(&transport, std::slice::from_ref(&first))
    };
    let listed = bundled.listings(&offline);
    assert_eq!(listed.len(), 1);
    let only = &listed[0];
    assert!(only.only_bundled);
    assert_eq!(only.version, v("1.0.3"));
    assert_eq!(
        (
            only.entry.id.as_str(),
            only.entry.title.as_str(),
            only.entry.category.as_str()
        ),
        ("probe", "Probe", "labs")
    );
    assert_eq!(only.entry.latest, v("1.0.3"));
    assert!(only.entry.versions.is_empty());
    assert_eq!(bundled.manifest("probe").unwrap().id, "probe");
    assert!(bundled.manifest("absent").is_err());

    let catalog = live_catalog(&transport, &[first, second]);
    let listed = bundled.listings(&catalog);
    assert!(!listed[0].only_bundled);
    assert_eq!(listed[0].entry, catalog.extensions[0]);
    assert_eq!(listed[0].version, v("1.0.3"));
}

#[test]
fn the_newer_of_the_catalog_and_the_bundle_is_offered() {
    assert_eq!(newest(Some(v("1.0.4")), Some(v("1.0.3"))), Some(v("1.0.4")));
    assert_eq!(newest(Some(v("1.0.2")), Some(v("1.0.3"))), Some(v("1.0.3")));
    assert_eq!(newest(None, Some(v("1.0.3"))), Some(v("1.0.3")));
    assert_eq!(newest(None, None), None);
}
