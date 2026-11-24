use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

use veronica_core::packaging::Packaging;
use veronica_host::launcher::{resolve, LaunchEnvironment, SNAP_VR};

fn environment(
    sandboxed: bool,
    current_exe: Option<PathBuf>,
    vars: &[(&str, &str)],
) -> LaunchEnvironment {
    LaunchEnvironment::from_env(sandboxed, current_exe, |name| {
        vars.iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| OsString::from(value))
    })
}

fn executable(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "#!/bin/sh\n").unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn flatpak_runs_vr_through_flatpak() {
    let launch = resolve(&environment(
        true,
        None,
        &[("FLATPAK_ID", "io.github.namannn04.Veronica")],
    ))
    .unwrap();
    assert_eq!(launch.packaging, Packaging::Flatpak);
    assert_eq!(launch.command, "flatpak");
    assert_eq!(
        launch.args,
        ["run", "--command=vr", "io.github.namannn04.Veronica"]
    );
}

#[test]
fn snap_uses_the_snap_command_alias() {
    let launch = resolve(&environment(
        false,
        None,
        &[("SNAP_NAME", "veronica"), ("SNAP", "/snap/veronica/42")],
    ))
    .unwrap();
    assert_eq!(launch.packaging, Packaging::Snap);
    assert_eq!(launch.command, SNAP_VR);
    assert!(launch.args.is_empty());
}

#[test]
fn appimage_is_reported_as_unsupported() {
    let unsupported = resolve(&environment(
        false,
        Some("/tmp/.mount_x/usr/bin/veronica".into()),
        &[("APPIMAGE", "/home/me/Veronica.AppImage")],
    ))
    .unwrap_err();
    assert_eq!(unsupported.packaging, Packaging::AppImage);
    assert!(unsupported.reason.contains("/home/me/Veronica.AppImage"));
}

#[test]
fn plain_install_resolves_the_real_vr_behind_a_symlink() {
    let root = tempfile::tempdir().unwrap();
    let real = root.path().join("opt/veronica/vr");
    executable(&real);
    let link = root.path().join("bin/vr");
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    symlink(&real, &link).unwrap();
    let launch = resolve(&environment(false, Some(link), &[])).unwrap();
    assert_eq!(launch.packaging, Packaging::Deb);
    assert_eq!(
        PathBuf::from(&launch.command),
        fs::canonicalize(&real).unwrap()
    );
    assert!(launch.args.is_empty());
}

#[test]
fn plain_install_finds_vr_beside_the_desktop_binary_then_on_path() {
    let root = tempfile::tempdir().unwrap();
    let sibling = root.path().join("usr/bin/vr");
    executable(&sibling);
    let desktop = root.path().join("usr/bin/veronica");
    let launch = resolve(&environment(false, Some(desktop), &[])).unwrap();
    assert_eq!(
        PathBuf::from(&launch.command),
        fs::canonicalize(&sibling).unwrap()
    );

    let on_path = root.path().join("path/vr");
    executable(&on_path);
    let path = format!("relative:{}", on_path.parent().unwrap().display());
    let elsewhere = root.path().join("elsewhere/veronica");
    let launch = resolve(&environment(false, Some(elsewhere), &[("PATH", &path)])).unwrap();
    assert_eq!(
        PathBuf::from(&launch.command),
        fs::canonicalize(&on_path).unwrap()
    );
}

#[test]
fn plain_install_without_vr_is_unsupported() {
    let root = tempfile::tempdir().unwrap();
    let not_executable = root.path().join("bin/vr");
    fs::create_dir_all(not_executable.parent().unwrap()).unwrap();
    fs::write(&not_executable, "").unwrap();
    let path = not_executable.parent().unwrap().display().to_string();
    let unsupported = resolve(&environment(
        false,
        Some(root.path().join("app/veronica")),
        &[("PATH", &path)],
    ))
    .unwrap_err();
    assert_eq!(unsupported.packaging, Packaging::Deb);
}
