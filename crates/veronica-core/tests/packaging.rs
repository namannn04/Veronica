use std::path::{Path, PathBuf};

const DEVELOPMENT_ONLY: [&str; 2] = ["install.sh", "package.json"];

fn repo_root() -> PathBuf {
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
        if !path.is_file() || DEVELOPMENT_ONLY.contains(&name) || name.ends_with(".test.mjs") {
            continue;
        }
        let shipped = matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("js" | "css" | "json" | "png")
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

#[test]
fn every_desktop_package_ships_the_bundled_extensions() {
    let root = repo_root();
    let read = |relative: &str| {
        std::fs::read_to_string(root.join(relative))
            .unwrap_or_else(|e| panic!("cannot read {relative}: {e}"))
    };
    let config: serde_json::Value =
        serde_json::from_str(&read("apps/desktop/src-tauri/tauri.conf.json"))
            .expect("valid Tauri configuration");
    assert_eq!(
        config["bundle"]["linux"]["deb"]["files"]["/usr/share/veronica/extensions"],
        "../../../packaging/bundled-extensions",
        "the Debian package and the AppImage built from it carry the bundled extensions"
    );
    let command = config["build"]["beforeBuildCommand"]
        .as_str()
        .expect("beforeBuildCommand");
    assert!(
        command.contains(
            "veronica-pack -- bundle --packages ../../target/ext --out ../../packaging/bundled-extensions"
        ),
        "every desktop build regenerates the bundled index, even with no packages"
    );
    assert!(read("packaging/flatpak/io.github.namannn04.Veronica.yml")
        .contains("cp -r usr/share/veronica/extensions /app/share/veronica/extensions"));
    assert!(read("packaging/snap/snapcraft.yaml").contains("      - usr/share\n"));
    assert!(read("packaging/ppa/debian/install-files.sh").contains("cp -R"));
    assert!(read("scripts/build-ppa-sources.sh").contains("packaging/bundled-extensions"));
    assert!(read(".gitignore").contains("/packaging/bundled-extensions/"));
}

#[test]
fn the_test_fixture_is_never_planned_into_a_package() {
    let plan = std::fs::read_to_string(repo_root().join("scripts/extensions-plan.sh"))
        .expect("scripts/extensions-plan.sh");
    assert!(plan.contains("excluded=\" ${PLAN_EXCLUDE-fixture} \""));
}

#[test]
fn the_main_webview_is_created_only_when_opened() {
    let config = repo_root().join("apps/desktop/src-tauri/tauri.conf.json");
    let document: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&config)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", config.display())),
    )
    .expect("valid Tauri configuration");
    let main = document["app"]["windows"]
        .as_array()
        .expect("window array")
        .iter()
        .find(|window| window["label"] == "main")
        .expect("main window");

    assert_eq!(
        main["create"], false,
        "autostart must not allocate a WebKit renderer until Veronica is opened"
    );
    assert!(
        document["app"].get("trayIcon").is_none(),
        "the tray is built in Rust; a configured tray would allocate a duplicate indicator"
    );
}

#[test]
fn the_debian_package_declares_the_update_check_runtime() {
    let config = repo_root().join("apps/desktop/src-tauri/tauri.conf.json");
    let document: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&config)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", config.display())),
    )
    .expect("valid Tauri configuration");
    let debian = &document["bundle"]["linux"]["deb"];
    let depends_on_curl = debian["depends"]
        .as_array()
        .expect("Debian dependency array")
        .iter()
        .any(|value| value.as_str().expect("string dependency") == "curl");
    let recommends_yt_dlp = debian["recommends"]
        .as_array()
        .expect("Debian recommendations array")
        .iter()
        .any(|value| value.as_str().expect("string recommendation") == "yt-dlp");

    assert!(
        depends_on_curl,
        "the update checker executes curl, so a clean Debian install must pull it in"
    );
    assert!(
        !recommends_yt_dlp,
        "Veronica has no download queue, so installing yt-dlp is an unrelated side effect"
    );
}

#[test]
fn every_release_surface_uses_the_workspace_version() {
    let root = repo_root();
    let expected = env!("CARGO_PKG_VERSION");
    let json_version = |relative: &str| {
        let path = root.join(relative);
        let value: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display())),
        )
        .unwrap_or_else(|e| panic!("invalid {}: {e}", path.display()));
        value["version"]
            .as_str()
            .unwrap_or_else(|| panic!("{} has no string version", path.display()))
            .to_string()
    };

    assert_eq!(json_version("apps/desktop/package.json"), expected);
    assert_eq!(
        json_version("apps/desktop/src-tauri/tauri.conf.json"),
        expected
    );

    let metadata: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("extension/metadata.json")).unwrap(),
    )
    .unwrap();
    let patch: u64 = expected.rsplit('.').next().unwrap().parse().unwrap();
    assert_eq!(metadata["version"].as_u64(), Some(patch));

    let appstream = std::fs::read_to_string(
        root.join("packaging/linux/io.github.namannn04.Veronica.metainfo.xml"),
    )
    .unwrap();
    assert!(appstream.contains(&format!("<release version=\"{expected}\"")));

    for relative in ["README.md", "docs/RUNNING.md"] {
        let document = std::fs::read_to_string(root.join(relative)).unwrap();
        assert!(
            document.contains(&format!("Veronica_{expected}_amd64")),
            "{relative} must show the current artifact name"
        );
    }
}

#[test]
fn public_installer_verifies_the_release_before_installing_it() {
    let script = std::fs::read_to_string(repo_root().join("install.sh")).expect("install.sh");

    assert!(script.contains("SHA256SUMS"));
    assert!(script.contains("sha256sum --check --status"));
    assert!(script.contains("apt install --reinstall --yes"));
    assert!(
        script.find("sha256sum --check --status").unwrap()
            < script.find("apt install --reinstall --yes").unwrap(),
        "the package must be verified before apt is allowed to install it"
    );
}

#[test]
fn github_pages_publishes_the_canonical_installer() {
    let root = repo_root();
    let workflow =
        std::fs::read_to_string(root.join(".github/workflows/pages.yml")).expect("Pages workflow");
    let builder =
        std::fs::read_to_string(root.join("scripts/build-pages.sh")).expect("Pages builder");
    let page = std::fs::read_to_string(root.join("site/index.html")).expect("landing page");

    assert!(workflow.contains("actions/deploy-pages@"));
    assert!(workflow.contains("scripts/build-pages.sh"));
    assert!(builder.contains("cp \"$repo_root/install.sh\" \"$destination/install\""));
    assert!(builder.contains("cmp --silent"));
    assert!(page.contains("https://veronica.namandadhich.in/install | bash"));
}

#[test]
fn tagged_releases_publish_every_documented_download() {
    let workflow = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .expect("release workflow");

    for required in ["amd64.deb", "amd64.AppImage", "SHA256SUMS"] {
        assert!(
            workflow.contains(required),
            "release workflow does not publish {required}"
        );
    }
    assert!(workflow.contains("GITHUB_REF_NAME"));
    assert!(workflow.contains("--verify-tag"));
}

#[test]
fn the_source_installer_copies_every_extension_file() {
    let script = std::fs::read_to_string(repo_root().join("extension/install.sh"))
        .expect("extension/install.sh");

    assert!(
        script.contains("*.js") && script.contains("*.json") && script.contains("*.css"),
        "install.sh must copy extension files by glob, not by an explicit list \
         that goes stale whenever a module is added"
    );

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
fn the_extension_supports_every_ubuntu_release_we_claim() {
    let path = repo_root().join("extension/metadata.json");
    let metadata: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display())),
    )
    .expect("valid extension metadata");
    let actual: Vec<&str> = metadata["shell-version"]
        .as_array()
        .expect("shell-version array")
        .iter()
        .map(|version| version.as_str().expect("string shell version"))
        .collect();

    assert_eq!(actual, ["46", "47", "48", "49", "50"]);
}
