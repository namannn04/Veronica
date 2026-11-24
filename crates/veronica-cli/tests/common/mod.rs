use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, PoisonError};

use serde_json::Value;

fn backend(id: &str) -> PathBuf {
    static BUILT: Mutex<BTreeMap<String, PathBuf>> = Mutex::new(BTreeMap::new());
    let mut built = BUILT.lock().unwrap_or_else(PoisonError::into_inner);
    built
        .entry(id.to_string())
        .or_insert_with(|| {
            let package = format!("veronica-ext-{id}");
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
            let output = Command::new(cargo)
                .args([
                    "build",
                    "--quiet",
                    "--package",
                    &package,
                    "--message-format=json",
                ])
                .stderr(Stdio::inherit())
                .output()
                .expect("cargo runs");
            assert!(output.status.success(), "building {package} failed");
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .filter(|message| message["target"]["name"] == package.as_str())
                .find_map(|message| message["executable"].as_str().map(PathBuf::from))
                .expect("cargo reports the backend executable")
        })
        .clone()
}

pub(crate) fn extension_source(root: &Path, id: &str) -> PathBuf {
    let source = root.join(format!("{id}-source"));
    fs::create_dir_all(source.join("bin")).unwrap();
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../extensions")
        .join(id)
        .join("manifest.json");
    fs::copy(manifest, source.join("manifest.json")).unwrap();
    let executable = source.join("bin/backend");
    fs::copy(backend(id), &executable).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    source
}
