mod common;

use std::process::{Command, Output, Stdio};

use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

struct Home {
    root: TempDir,
}

impl Home {
    fn with_calendar() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&AppDirectories::with_env(
            home.root.path(),
            EnvOverrides::default(),
        ));
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "calendar"))
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
fn the_calendar_commands_come_from_the_installed_extension() {
    let home = Home::with_calendar();
    let help = home.vr(&["calendar", "--help"]);
    assert_eq!(help.status.code(), Some(0));
    let text = stdout(&help);
    assert!(
        text.contains("Your agenda, from every configured calendar"),
        "{text}"
    );
    for leaf in ["list", "next"] {
        assert!(text.contains(leaf), "{text}");
    }
    let list = stdout(&home.vr(&["calendar", "list", "--help"]));
    assert!(list.contains("Usage: vr calendar list [OPTIONS]"), "{list}");
    assert!(list.contains("--days <DAYS>"), "{list}");
    assert!(list.contains("--all"), "{list}");
    let aliased = stdout(&home.vr(&["calendar", "agenda", "--help"]));
    assert!(aliased.contains("The agenda, grouped by day"), "{aliased}");
}

#[test]
fn without_the_extension_the_calendar_command_is_gone() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_vr"))
        .env("HOME", root.path())
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_STATE_HOME")
        .args(["calendar", "next"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_ne!(output.status.code(), Some(0));
}
