use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use serde_json::Value;

pub(crate) fn fixture_backend() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let output = Command::new(cargo)
            .args([
                "build",
                "--quiet",
                "--package",
                "veronica-ext-fixture",
                "--message-format=json",
            ])
            .stderr(Stdio::inherit())
            .output()
            .expect("cargo runs");
        assert!(output.status.success(), "building the fixture failed");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|message| message["target"]["name"] == "veronica-ext-fixture")
            .find_map(|message| message["executable"].as_str().map(PathBuf::from))
            .expect("cargo reports the fixture executable")
    })
}

pub(crate) fn fixture_source(root: &Path) -> PathBuf {
    let source = root.join("fixture-source");
    fs::create_dir_all(source.join("bin")).unwrap();
    let manifest =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions/fixture/manifest.json");
    fs::copy(manifest, source.join("manifest.json")).unwrap();
    let backend = source.join("bin/backend");
    fs::copy(fixture_backend(), &backend).unwrap();
    fs::set_permissions(&backend, fs::Permissions::from_mode(0o755)).unwrap();
    source
}
