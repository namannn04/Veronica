mod common;

use std::fs;
use std::process::{Command, Output, Stdio};

use serde_json::{json, Value};
use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

struct Home {
    root: TempDir,
}

impl Home {
    fn with_usage() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&AppDirectories::with_env(
            home.root.path(),
            EnvOverrides::default(),
        ));
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "usage"))
            .unwrap();
        home
    }

    fn collected(self) -> Self {
        let directory = self.root.path().join(".local/share/veronica/usage");
        fs::create_dir_all(&directory).unwrap();
        let document = json!({
            "schemaVersion": veronica_usage::SCHEMA_VERSION,
            "generatedAt": "2026-11-20T00:00:00Z",
            "sources": ["cli"],
        });
        fs::write(directory.join("usage.json"), document.to_string()).unwrap();
        self
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
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
}

fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn document(output: &Output) -> Value {
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn both_roots_and_their_long_help_come_from_the_extension() {
    let home = Home::with_usage();
    let usage = text(&home.vr(&["usage", "--help"]));
    assert!(usage.contains("Agent usage: totals, limits"), "{usage}");
    let alerts = text(&home.vr(&["alerts", "--help"]));
    assert!(alerts.contains("Rate-limit alerts"), "{alerts}");
    let long = text(&home.vr(&["usage", "alerts", "--help"]));
    assert!(long.contains("A dry run"), "{long}");
    let short = text(&home.vr(&["usage", "alerts", "-h"]));
    assert!(!short.contains("A dry run"), "{short}");
}

#[test]
fn collected_usage_is_summarised_through_the_backend() {
    let home = Home::with_usage().collected();
    let summary = document(&home.vr(&["--json", "usage", "summary"]));
    assert_eq!(summary["generatedAt"], "2026-11-20T00:00:00Z");
    assert_eq!(summary["activeDays"], 0);
    let state = document(&home.vr(&["alerts", "state", "--json"]));
    assert_eq!(state["sessionLevel"], "green");
}

#[test]
fn missing_usage_points_at_refresh() {
    let home = Home::with_usage();
    let output = home.vr(&["usage", "summary"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("vr usage refresh"));
}
