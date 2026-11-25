mod common;

use std::process::{Command, Output, Stdio};

use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

struct Home {
    root: TempDir,
}

impl Home {
    fn with_attention() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&AppDirectories::with_env(
            home.root.path(),
            EnvOverrides::default(),
        ));
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "attention"))
            .unwrap();
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
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn document(output: &Output) -> serde_json::Value {
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn the_attention_commands_come_from_the_installed_extension() {
    let home = Home::with_attention();
    let help = home.vr(&["attention", "--help"]);
    assert_eq!(help.status.code(), Some(0));
    let text = stdout(&help);
    assert!(
        text.contains("Attention focus sessions and local attention history"),
        "{text}"
    );
    for leaf in ["status", "focus", "history", "summary", "configure"] {
        assert!(text.contains(leaf), "{text}");
    }
    assert!(!text.contains("record"), "{text}");
    let start = stdout(&home.vr(&["attention", "focus", "start", "--help"]));
    assert!(start.contains("--for <DURATION_SECONDS>"), "{start}");
    assert!(start.contains("[default: 45m]"), "{start}");
}

#[test]
fn a_focus_session_runs_through_the_backend() {
    let home = Home::with_attention();
    let started = document(&home.vr(&[
        "--json",
        "attention",
        "focus",
        "start",
        "--name",
        "Write",
        "--for",
        "25m",
    ]));
    assert_eq!(started["name"], "Write");
    assert_eq!(started["plannedDurationSeconds"], 1500);
    let status = document(&home.vr(&["--json", "attention", "status"]));
    assert_eq!(status["active"]["name"], "Write");
    let stopped = document(&home.vr(&["--json", "attention", "focus", "stop"]));
    assert_eq!(stopped["name"], "Write");
    let history = document(&home.vr(&["--json", "attention", "history"]));
    assert_eq!(history.as_array().unwrap().len(), 1);
}

#[test]
fn configuring_tracking_shares_it_with_the_shell() {
    let home = Home::with_attention();
    document(&home.vr(&[
        "--json",
        "attention",
        "configure",
        "--enabled",
        "true",
        "--privacy",
        "detailed",
    ]));
    let recorded =
        document(&home.vr(&["--json", "attention", "record", "--application", "Terminal"]));
    assert_eq!(recorded["application"], "Terminal");
    let settings =
        std::fs::read_to_string(home.root.path().join(".config/veronica/settings.json")).unwrap();
    let settings: serde_json::Value = serde_json::from_str(&settings).unwrap();
    assert_eq!(settings["attentionTrackingEnabled"], true);
    assert_eq!(settings["attentionPrivacy"], "detailed");
}
