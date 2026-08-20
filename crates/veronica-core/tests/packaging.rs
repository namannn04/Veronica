//! Packaging checks.
//!
//! The Debian package lists the shell extension's files explicitly, so adding a
//! new one to `extension/` without listing it produces an installed extension
//! that fails to import at runtime — with nothing wrong at build time. This
//! catches that at test time instead.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/veronica-core.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repository root")
        .to_path_buf()
}

#[test]
fn every_extension_file_is_listed_in_the_debian_package() {
    let root = repo_root();
    let extension_dir = root.join("extension");
    let config = root.join("apps/desktop/src-tauri/tauri.conf.json");

    let manifest = std::fs::read_to_string(&config)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", config.display()));

    let mut missing = Vec::new();
    for entry in std::fs::read_dir(&extension_dir).expect("extension directory") {
        let path = entry.expect("directory entry").path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // install.sh is for source checkouts; the package does not ship it.
        if name == "install.sh" || !path.is_file() {
            continue;
        }
        let shipped = matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("js") | Some("css") | Some("json")
        );
        if shipped && !manifest.contains(&format!("extension/{name}")) {
            missing.push(name.to_string());
        }
    }

    assert!(
        missing.is_empty(),
        "these extension files are not in the Debian package's file list, so an \
         installed extension would fail to import them: {missing:?}"
    );
}

#[test]
fn the_extension_declares_the_running_shell_version() {
    let metadata = std::fs::read_to_string(repo_root().join("extension/metadata.json"))
        .expect("extension metadata");
    // GNOME refuses to load an extension that does not name the running series.
    assert!(
        metadata.contains("\"50\""),
        "metadata.json must list shell-version 50"
    );
}
