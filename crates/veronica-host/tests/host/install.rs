use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::lock::ExtensionsLock;
use veronica_host::registry::{Origin, Registry};
use veronica_host::{HostIdentity, Installer, Layout};
use veronica_protocol::manifest::Target;
use veronica_protocol::PackageVersion;

use crate::common::{manifest, manifest_with, package, package_tree, write_tree};

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

fn install(installer: &Installer, id: &str, version: &str) -> anyhow::Result<()> {
    let version = v(version);
    let bytes = package(
        &manifest(id, &version.to_string()),
        version.to_string().as_bytes(),
    );
    installer
        .install_archive(bytes.as_slice(), id, version)
        .map(drop)
}

fn link(path: &Path) -> Option<String> {
    fs::read_link(path)
        .ok()
        .map(|target| target.to_string_lossy().into_owned())
}

fn current(installer: &Installer, id: &str) -> Option<String> {
    link(&installer.layout().current_link(id))
}

fn previous(installer: &Installer, id: &str) -> Option<String> {
    link(&installer.layout().previous_link(id))
}

fn versions_on_disk(installer: &Installer, id: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(installer.layout().package_dir(id))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name != "current" && name != "previous")
        .collect();
    names.sort();
    names
}

fn staging_is_empty(installer: &Installer) -> bool {
    fs::read_dir(installer.layout().staging_dir())
        .map_or(true, |mut entries| entries.next().is_none())
}

#[test]
fn install_upgrade_prune_and_rollback() {
    let (_home, installer) = installer();
    install(&installer, "probe", "1.0.1").unwrap();
    assert_eq!(current(&installer, "probe").as_deref(), Some("1.0.1"));
    assert_eq!(previous(&installer, "probe"), None);
    let backend = installer.layout().current_link("probe").join("bin/backend");
    assert_eq!(fs::read(&backend).unwrap(), b"1.0.1");
    assert_eq!(
        fs::metadata(&backend).unwrap().permissions().mode() & 0o7777,
        0o755
    );

    install(&installer, "probe", "1.0.2").unwrap();
    install(&installer, "probe", "1.0.3").unwrap();
    assert_eq!(current(&installer, "probe").as_deref(), Some("1.0.3"));
    assert_eq!(previous(&installer, "probe").as_deref(), Some("1.0.2"));
    assert_eq!(versions_on_disk(&installer, "probe"), ["1.0.2", "1.0.3"]);
    let entry = installer.registry().unwrap().extensions["probe"].clone();
    assert_eq!(
        (entry.version, entry.previous),
        (v("1.0.3"), Some(v("1.0.2")))
    );
    assert_eq!(entry.origin, Origin::Catalog);
    assert!(entry.enabled);

    let rolled = installer.rollback("probe").unwrap();
    assert_eq!(
        (rolled.version, rolled.previous),
        (v("1.0.2"), Some(v("1.0.3")))
    );
    assert_eq!(current(&installer, "probe").as_deref(), Some("1.0.2"));
    assert_eq!(previous(&installer, "probe").as_deref(), Some("1.0.3"));
    assert_eq!(fs::read(&backend).unwrap(), b"1.0.2");
    assert_eq!(installer.registry().unwrap().extensions["probe"], rolled);

    installer.rollback("probe").unwrap();
    assert_eq!(current(&installer, "probe").as_deref(), Some("1.0.3"));
    assert!(staging_is_empty(&installer));
}

#[test]
fn reinstalling_the_current_version_keeps_previous() {
    let (_home, installer) = installer();
    install(&installer, "probe", "1.0.1").unwrap();
    install(&installer, "probe", "1.0.2").unwrap();
    install(&installer, "probe", "1.0.2").unwrap();
    assert_eq!(current(&installer, "probe").as_deref(), Some("1.0.2"));
    assert_eq!(previous(&installer, "probe").as_deref(), Some("1.0.1"));
    assert_eq!(versions_on_disk(&installer, "probe"), ["1.0.1", "1.0.2"]);
}

#[test]
fn rollback_needs_a_previous_version() {
    let (_home, installer) = installer();
    assert!(installer.rollback("probe").is_err());
    install(&installer, "probe", "1.0.1").unwrap();
    assert!(installer.rollback("probe").is_err());
}

#[test]
fn packages_that_do_not_match_the_request_or_host_are_refused() {
    let (_home, installer) = installer();
    let cases = [
        ("other", manifest("other", "1.0.0")),
        ("probe", manifest("probe", "2.0.0")),
        ("probe", manifest("probe", "1.0")),
        ("probe", manifest("probe", "1.0.1")),
        (
            "probe",
            manifest_with("probe", "1.0.0", ">=0.6.0", 1, "linux-x86_64"),
        ),
        (
            "probe",
            manifest_with("probe", "1.0.0", "not a range", 1, "linux-x86_64"),
        ),
        (
            "probe",
            manifest_with("probe", "1.0.0", ">=0.1.0", 2, "linux-x86_64"),
        ),
        (
            "probe",
            manifest_with("probe", "1.0.0", ">=0.1.0", 1, "linux-aarch64"),
        ),
    ];
    for (label, manifest) in cases {
        let bytes = package(&manifest, b"x");
        let result = installer.install_archive(bytes.as_slice(), "probe", v("1.0.0"));
        assert!(result.is_err(), "{label} was accepted");
        assert!(!installer.layout().package_dir("probe").exists());
        assert!(staging_is_empty(&installer));
    }
    assert!(installer.registry().unwrap().extensions.is_empty());
}

#[test]
fn remove_keeps_data_unless_purged() {
    let (_home, installer) = installer();
    for id in ["kept", "purged"] {
        install(&installer, id, "1.0.0").unwrap();
        write_tree(
            &installer.layout().data_dir(id),
            &[("state.json", b"{}", 0o644)],
        );
    }
    installer.remove("kept", false).unwrap();
    installer.remove("purged", true).unwrap();
    for id in ["kept", "purged"] {
        assert!(!installer.layout().package_dir(id).exists());
    }
    assert!(installer
        .layout()
        .data_dir("kept")
        .join("state.json")
        .is_file());
    assert!(!installer.layout().data_dir("purged").exists());
    assert!(installer.registry().unwrap().extensions.is_empty());
    assert!(installer.remove("kept", false).is_err());
}

#[test]
fn a_truncated_download_leaves_no_partial_state() {
    let (_home, installer) = installer();
    install(&installer, "probe", "1.0.1").unwrap();
    let before = installer.registry().unwrap();
    let bytes = package(&manifest("probe", "1.0.2"), &vec![7; 64 * 1024]);
    let truncated = &bytes[..bytes.len() / 2];
    assert!(installer
        .install_archive(truncated, "probe", v("1.0.2"))
        .is_err());
    assert_eq!(installer.registry().unwrap(), before);
    assert_eq!(versions_on_disk(&installer, "probe"), ["1.0.1"]);
    assert_eq!(current(&installer, "probe").as_deref(), Some("1.0.1"));
    assert!(staging_is_empty(&installer));
}

#[test]
fn a_failed_registry_write_restores_the_previous_install() {
    let (_home, installer) = installer();
    install(&installer, "probe", "1.0.1").unwrap();
    install(&installer, "probe", "1.0.2").unwrap();
    let before = installer.registry().unwrap();
    let state = installer
        .layout()
        .installed_file()
        .parent()
        .unwrap()
        .to_path_buf();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o555)).unwrap();
    let writable = fs::write(state.join("probe"), b"").is_ok();
    let result = install(&installer, "probe", "1.0.3");
    fs::set_permissions(&state, fs::Permissions::from_mode(0o755)).unwrap();
    if writable {
        return;
    }
    assert!(result.is_err());
    assert_eq!(installer.registry().unwrap(), before);
    assert_eq!(current(&installer, "probe").as_deref(), Some("1.0.2"));
    assert_eq!(previous(&installer, "probe").as_deref(), Some("1.0.1"));
    assert_eq!(versions_on_disk(&installer, "probe"), ["1.0.1", "1.0.2"]);
}

#[test]
fn leftovers_from_a_crashed_install_are_cleared() {
    let (_home, installer) = installer();
    install(&installer, "probe", "1.0.1").unwrap();
    let staging = installer.layout().staging_dir();
    write_tree(&staging, &[("stage-dead/tree/manifest.json", b"{}", 0o644)]);
    let package_dir = installer.layout().package_dir("probe");
    std::os::unix::fs::symlink("1.0.1", package_dir.join(".current.tmp")).unwrap();
    fs::create_dir(package_dir.join("9.9.9")).unwrap();
    install(&installer, "probe", "1.0.2").unwrap();
    assert!(staging_is_empty(&installer));
    assert_eq!(versions_on_disk(&installer, "probe"), ["1.0.1", "1.0.2"]);
}

#[test]
fn concurrent_installs_serialize_on_the_lock() {
    let (_home, installer) = installer();
    std::thread::scope(|scope| {
        for id in ["alpha", "bravo", "charlie", "delta"] {
            let installer = installer.clone();
            scope.spawn(move || {
                for patch in 1..=3 {
                    install(&installer, id, &format!("1.0.{patch}")).unwrap();
                }
            });
        }
    });
    let registry = installer.registry().unwrap();
    assert_eq!(registry.extensions.len(), 4);
    for (id, entry) in &registry.extensions {
        assert_eq!(
            (entry.version, entry.previous),
            (v("1.0.3"), Some(v("1.0.2")))
        );
        assert_eq!(current(&installer, id).as_deref(), Some("1.0.3"));
    }
}

#[test]
fn an_install_waits_for_another_holder_of_the_lock() {
    let (_home, installer) = installer();
    let held = ExtensionsLock::acquire(&installer.layout().lock_file()).unwrap();
    let (done, finished) = mpsc::channel();
    std::thread::scope(|scope| {
        let installer = installer.clone();
        scope.spawn(move || {
            install(&installer, "probe", "1.0.1").unwrap();
            done.send(()).unwrap();
        });
        assert!(finished.recv_timeout(Duration::from_millis(300)).is_err());
        drop(held);
        finished.recv_timeout(Duration::from_secs(30)).unwrap();
    });
    assert_eq!(current(&installer, "probe").as_deref(), Some("1.0.1"));
}

#[test]
fn a_local_directory_installs_as_a_sideload() {
    let (_home, installer) = installer();
    let source = tempfile::tempdir().unwrap();
    package_tree(source.path(), &manifest("probe", "0.3"), b"dev build");
    let entry = installer.install_sideload(source.path()).unwrap();
    assert_eq!(entry.origin, Origin::Sideload);
    assert_eq!(entry.version, v("0.3.0"));
    let tree = installer.layout().current_link("probe");
    assert!(tree.join("files.json").is_file());
    assert_eq!(fs::read(tree.join("bin/backend")).unwrap(), b"dev build");
    assert_eq!(
        fs::metadata(tree.join("bin/backend"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o755
    );

    fs::write(source.path().join("bin/backend"), b"rebuilt").unwrap();
    installer.install_sideload(source.path()).unwrap();
    assert_eq!(fs::read(tree.join("bin/backend")).unwrap(), b"rebuilt");
    assert_eq!(previous(&installer, "probe"), None);
    assert_eq!(
        Registry::load(&installer.layout().installed_file())
            .unwrap()
            .extensions["probe"]
            .origin,
        Origin::Sideload
    );

    let packed = tempfile::tempdir().unwrap();
    package_tree(packed.path(), &manifest("probe", "0.3.9"), b"packed");
    assert_eq!(
        installer.install_sideload(packed.path()).unwrap().version,
        v("0.3.0")
    );

    std::os::unix::fs::symlink("/etc/passwd", source.path().join("leak")).unwrap();
    assert!(installer.install_sideload(source.path()).is_err());
    assert!(staging_is_empty(&installer));
}
