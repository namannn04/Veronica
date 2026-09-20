//! Packaging checks.
//!
//! The Debian package lists the shell extension's files explicitly, so adding a
//! new one to `extension/` without listing it produces an installed extension
//! that fails to import at runtime — with nothing wrong at build time. This
//! catches that at test time instead.

use std::path::{Path, PathBuf};

/// Files that live in `extension/` for tooling's benefit and are not part of the
/// extension GNOME loads.
const DEVELOPMENT_ONLY: [&str; 2] = ["install.sh", "package.json"];

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
        // Development-only files: the installer script, the ESM declaration
        // that lets node and editors read this directory, and its tests. GNOME
        // loads none of them.
        if !path.is_file() || DEVELOPMENT_ONLY.contains(&name) || name.ends_with(".test.mjs") {
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
fn every_desktop_release_rebuilds_the_cli_it_packages() {
    let config = repo_root().join("apps/desktop/src-tauri/tauri.conf.json");
    let document: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&config)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", config.display())),
    )
    .expect("valid Tauri configuration");
    let command = document["build"]["beforeBuildCommand"]
        .as_str()
        .expect("beforeBuildCommand");

    assert!(
        command.contains("cargo build --release -p veronica-cli"),
        "the Debian bundle copies target/release/vr, so its build must be part of every Tauri release"
    );
}

/// The same failure by the other route: `install.sh` is what a source checkout
/// uses, and an explicit file list there goes stale silently too.
#[test]
fn the_source_installer_copies_every_extension_file() {
    let script = std::fs::read_to_string(repo_root().join("extension/install.sh"))
        .expect("extension/install.sh");

    // Globbing by extension is the only form that cannot go stale. If the
    // script ever goes back to naming files one by one, this fails and says so.
    assert!(
        script.contains("*.js") && script.contains("*.json") && script.contains("*.css"),
        "install.sh must copy extension files by glob, not by an explicit list \
         that goes stale whenever a module is added"
    );

    // The glob would otherwise sweep in the development-only files too.
    for name in DEVELOPMENT_ONLY {
        if name == "install.sh" {
            continue;
        }
        assert!(
            script.contains(name),
            "install.sh must exclude {name}, which GNOME does not load"
        );
    }
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
