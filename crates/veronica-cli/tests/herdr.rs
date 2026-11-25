mod common;

use std::process::{Command, Output, Stdio};

use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

struct Home {
    root: TempDir,
}

impl Home {
    fn with_herdr() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&AppDirectories::with_env(
            home.root.path(),
            EnvOverrides::default(),
        ));
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "herdr"))
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
fn the_herdr_commands_come_from_the_installed_extension() {
    let home = Home::with_herdr();
    let help = home.vr(&["herdr", "--help"]);
    assert_eq!(help.status.code(), Some(0));
    let text = stdout(&help);
    assert!(text.contains("The live Herdr agent board"), "{text}");
    for leaf in ["board", "sessions", "attach"] {
        assert!(text.contains(leaf), "{text}");
    }
    let attach = stdout(&home.vr(&["herdr", "attach", "--help"]));
    assert!(
        attach.contains("Usage: vr herdr attach [OPTIONS] <SESSION> [PANE_ID]"),
        "{attach}"
    );
    assert!(attach.contains("Printed rather than run"), "{attach}");
    let board = stdout(&home.vr(&["herdr", "ls", "--help"]));
    assert!(board.contains("--kind <KIND>"), "{board}");
}
