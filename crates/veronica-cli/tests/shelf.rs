use serde_json::Value;
use std::{path::Path, process::Command};
fn vr(root: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_vr"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_RUNTIME_DIR", root.join("runtime"))
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
    assert!(vr(root, &["shelf", "remove", id])["entries"]
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
    vr(root, &["shelf", "clear"]);
    assert!(vr(root, &["shelf", "list"])["entries"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(original.exists());
}
