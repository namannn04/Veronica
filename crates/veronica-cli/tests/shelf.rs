mod common;

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

fn install_shelf(root: &Path) {
    let layout = Layout::new(&AppDirectories::with_env(root, EnvOverrides::default()));
    Installer::new(layout, HostIdentity::current())
        .install_sideload(&common::extension_source(root, "shelf"))
        .unwrap();
}

fn vr(root: &Path, args: &[&str]) -> Value {
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
    let output = command
        .env("HOME", root)
        .arg("--json")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn notch_commands_share_persistent_copies_and_do_not_resurrect_removed_legacy_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    install_shelf(root);
    let original = root.join("with spaces.txt");
    std::fs::write(&original, b"original").unwrap();
    let result = vr(root, &["shelf", "add", original.to_str().unwrap()]);
    let entry = &result["entries"][0];
    let saved = entry["path"].as_str().unwrap();
    let id = entry["id"].as_str().unwrap();
    assert_ne!(saved, original.to_str().unwrap());
    std::fs::write(&original, b"changed").unwrap();
    assert_eq!(std::fs::read(saved).unwrap(), b"original");
    assert_eq!(
        vr(root, &["shelf", "list"])["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(vr(root, &["shelf", "remove", id, "--yes"])["entries"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(std::fs::read(&original).unwrap(), b"changed");
    vr(
        root,
        &[
            "config",
            "set",
            "notchShelfFiles",
            &serde_json::json!([original]).to_string(),
        ],
    );
    assert_eq!(
        vr(root, &["shelf", "list"])["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    vr(root, &["shelf", "clear", "--yes"]);
    assert!(vr(root, &["shelf", "list"])["entries"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(original.exists());
}
