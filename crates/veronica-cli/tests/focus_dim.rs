mod common;

use std::process::{Command, Output, Stdio};

use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

struct Home {
    root: TempDir,
}

impl Home {
    fn with_focus_dim() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&AppDirectories::with_env(
            home.root.path(),
            EnvOverrides::default(),
        ));
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "focus-dim"))
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
fn the_focus_dim_commands_come_from_the_installed_extension() {
    let home = Home::with_focus_dim();
    let help = stdout(&home.vr(&["focus-dim", "--help"]));
    assert!(
        help.contains("Focus Dim: darken everything behind the window you are working in"),
        "{help}"
    );
    for leaf in ["status", "on", "off", "intensity", "fade", "mode"] {
        assert!(help.contains(leaf), "{help}");
    }
}

#[test]
fn settings_written_through_the_backend_reach_the_shell() {
    let home = Home::with_focus_dim();
    let on = document(&home.vr(&["--json", "focus-dim", "on"]));
    assert_eq!(on["enabled"], true);
    let dimmer = document(&home.vr(&["--json", "focus-dim", "intensity", "60"]));
    assert_eq!(dimmer["intensityPercent"], 60);
    let settings =
        std::fs::read_to_string(home.root.path().join(".config/veronica/settings.json")).unwrap();
    let settings: serde_json::Value = serde_json::from_str(&settings).unwrap();
    assert_eq!(settings["focusDimEnabled"], true);
    let wrong = home.vr(&["--json", "focus-dim", "mode", "sideways"]);
    assert_ne!(wrong.status.code(), Some(0));
}
