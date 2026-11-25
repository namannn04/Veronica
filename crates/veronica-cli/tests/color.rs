mod common;

use std::process::{Command, Output, Stdio};

use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

struct Home {
    root: TempDir,
}

impl Home {
    fn with_color() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&AppDirectories::with_env(
            home.root.path(),
            EnvOverrides::default(),
        ));
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "color"))
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

#[test]
fn the_color_commands_come_from_the_installed_extension() {
    let home = Home::with_color();
    let help = home.vr(&["color", "--help"]);
    assert_eq!(help.status.code(), Some(0));
    let text = stdout(&help);
    assert!(
        text.contains("Sample a colour from the screen and keep a swatch history"),
        "{text}"
    );
    for leaf in ["pick", "list", "copy", "show", "remove", "clear"] {
        assert!(text.contains(leaf), "{text}");
    }
    let pick = stdout(&home.vr(&["color", "pick", "--help"]));
    assert!(pick.contains("--format <FORMAT>"), "{pick}");
    assert!(pick.contains("--no-copy"), "{pick}");
}

#[test]
fn the_swatch_history_is_read_through_the_backend() {
    let home = Home::with_color();
    let listed = home.vr(&["--json", "color", "list"]);
    assert_eq!(
        listed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(document, serde_json::json!([]));
    let aliased = home.vr(&["color", "ls"]);
    assert_eq!(stdout(&aliased).trim(), "no colours picked yet");
    let missing = home.vr(&["color", "show", "7"]);
    assert_ne!(missing.status.code(), Some(0));
}
