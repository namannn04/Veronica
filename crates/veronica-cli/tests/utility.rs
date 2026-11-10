use serde_json::Value;
use std::{path::Path, process::Command};
fn run(root: &Path, args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_vr"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
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
    assert!(jpg["artifact"].as_str().unwrap().ends_with(".jpg"));
    let detected = run(
        root.path(),
        &["utility", "image", jpg["artifact"].as_str().unwrap()],
    );
    assert_eq!(detected["format"], "JPG");
    assert!(detected["artifact"].is_null());
    assert!(Path::new(jpg["copyPath"].as_str().unwrap()).exists());
}
