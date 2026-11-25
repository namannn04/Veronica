mod common;

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

fn install_tools(root: &Path) {
    let layout = Layout::new(&AppDirectories::with_env(root, EnvOverrides::default()));
    Installer::new(layout, HostIdentity::current())
        .install_sideload(&common::extension_source(root, "utility"))
        .unwrap();
}

fn run(root: &Path, args: &[&str]) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vr"));
    for key in [
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "XDG_STATE_HOME",
        "XDG_RUNTIME_DIR",
    ] {
        command.env_remove(key);
    }
    let out = command
        .env("HOME", root)
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn inline_qr_and_conversion_return_private_small_previews_and_real_exports() {
    let root = tempfile::tempdir().unwrap();
    install_tools(root.path());
    let qr = run(root.path(), &["utility", "qr", "https://example.com/notch"]);
    let artifact = qr["artifact"].as_str().unwrap();
    assert!(Path::new(artifact).exists());
    assert!(
        std::fs::metadata(qr["preview"].as_str().unwrap())
            .unwrap()
            .len()
            < 100_000
    );
    let jpg = run(root.path(), &["utility", "image", artifact, "jpg"]);
    assert_eq!(jpg["format"], "PNG");
    assert_eq!(
        Path::new(jpg["artifact"].as_str().unwrap())
            .extension()
            .and_then(std::ffi::OsStr::to_str),
        Some("jpg")
    );
    let detected = run(
        root.path(),
        &["utility", "image", jpg["artifact"].as_str().unwrap()],
    );
    assert_eq!(detected["format"], "JPG");
    assert!(detected["artifact"].is_null());
    assert!(Path::new(jpg["copyPath"].as_str().unwrap()).exists());
}
