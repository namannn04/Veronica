mod common;

use std::process::{Command, Output, Stdio};

use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

struct Home {
    root: TempDir,
}

impl Home {
    fn with_keystroke() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&AppDirectories::with_env(
            home.root.path(),
            EnvOverrides::default(),
        ));
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(
                home.root.path(),
                "keystroke-highlight",
            ))
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
fn the_keystroke_commands_come_from_the_installed_extension() {
    let home = Home::with_keystroke();
    let help = stdout(&home.vr(&["keystroke-highlight", "--help"]));
    assert!(
        help.contains("Keystroke Highlight: show each key press on screen for demos"),
        "{help}"
    );
    for leaf in [
        "status", "on", "off", "enable", "disable", "duration", "position",
    ] {
        assert!(help.contains(leaf), "{help}");
    }
    let aliased = stdout(&home.vr(&["keys", "--help"]));
    assert!(aliased.contains("position"), "{aliased}");
}

#[test]
fn settings_written_through_the_backend_reach_the_shell() {
    let home = Home::with_keystroke();
    let on = document(&home.vr(&["--json", "keys", "on"]));
    assert_eq!(on["running"], true);
    let top = document(&home.vr(&["--json", "keystroke-highlight", "position", "top"]));
    assert_eq!(top["position"], "top");
    let settings =
        std::fs::read_to_string(home.root.path().join(".config/veronica/settings.json")).unwrap();
    let settings: serde_json::Value = serde_json::from_str(&settings).unwrap();
    assert_eq!(settings["keystrokeHighlightActive"], true);
}
