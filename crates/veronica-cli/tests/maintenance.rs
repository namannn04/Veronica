mod common;

use std::fs;
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

struct Home {
    root: TempDir,
}

impl Home {
    fn with_maintenance() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&AppDirectories::with_env(
            home.root.path(),
            EnvOverrides::default(),
        ));
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "maintenance"))
            .unwrap();
        let pip = home.root.path().join(".cache/pip/wheels");
        fs::create_dir_all(&pip).unwrap();
        fs::write(pip.join("blob"), vec![7_u8; 5000]).unwrap();
        let modules = home.root.path().join("proj/node_modules/left-pad");
        fs::create_dir_all(&modules).unwrap();
        fs::write(modules.join("index.js"), vec![1_u8; 3000]).unwrap();
        home
    }

    fn vr(&self, args: &[&str]) -> Output {
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
        command
            .env("HOME", self.root.path())
            .env("NO_COLOR", "1")
            .current_dir(self.root.path())
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
}

fn out(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn a_relative_root_is_read_from_the_callers_directory() {
    let home = Home::with_maintenance();
    let scanned = out(&home.vr(&["cleaner", "scan", "--root", "proj"]));
    assert!(scanned.contains("proj/node_modules"), "{scanned}");
    assert!(scanned.contains(".cache/pip"), "{scanned}");
}

#[test]
fn cleaning_previews_the_plan_and_changes_nothing_without_yes() {
    let home = Home::with_maintenance();
    let refused = home.vr(&["cleaner", "clean"]);
    assert_eq!(refused.status.code(), Some(5));
    let text = out(&refused);
    assert!(text.contains("4.9 KB  pip"), "{text}");
    assert!(text.ends_with("Nothing was changed. To go ahead, run:\n  vr cleaner clean --yes\n"));
    assert!(home.root.path().join(".cache/pip/wheels/blob").exists());

    let previewed: Value =
        serde_json::from_slice(&home.vr(&["--json", "cleaner", "clean"]).stdout).unwrap();
    assert_eq!(previewed["confirmationRequired"], true);
    assert_eq!(previewed["preview"]["totalBytes"], 5000);
    assert_eq!(previewed["rerun"], "vr --json cleaner clean --yes");
}

#[test]
fn package_sources_and_categories_answer_through_the_backend() {
    let home = Home::with_maintenance();
    let sources: Value =
        serde_json::from_slice(&home.vr(&["--json", "pkg", "sources"]).stdout).unwrap();
    assert_eq!(sources.as_array().unwrap().len(), 3);
    let categories: Value =
        serde_json::from_slice(&home.vr(&["--json", "cleaner", "ls"]).stdout).unwrap();
    assert!(categories
        .as_array()
        .unwrap()
        .iter()
        .any(|category| category["id"] == "nodeModules"));
}
