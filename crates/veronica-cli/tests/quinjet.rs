mod common;

use std::process::{Command, Output, Stdio};

use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

struct Home {
    root: TempDir,
}

impl Home {
    fn with_quinjet() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&AppDirectories::with_env(
            home.root.path(),
            EnvOverrides::default(),
        ));
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "quinjet"))
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
fn the_quinjet_commands_come_from_the_installed_extension() {
    let home = Home::with_quinjet();
    let help = home.vr(&["quinjet", "--help"]);
    assert_eq!(help.status.code(), Some(0));
    let text = stdout(&help);
    assert!(
        text.contains("Quinjet: discover and open review workspaces"),
        "{text}"
    );
    for leaf in ["projects", "worktrees", "open", "launch"] {
        assert!(text.contains(leaf), "{text}");
    }
    let open = stdout(&home.vr(&["quinjet", "open", "--help"]));
    assert!(
        open.contains("Usage: vr quinjet open [OPTIONS] <PATH>"),
        "{open}"
    );
    assert!(open.contains("[default: local]"), "{open}");
}

#[test]
fn an_unknown_machine_is_reported_by_the_backend() {
    let home = Home::with_quinjet();
    let failed = home.vr(&["--json", "quinjet", "projects", "--machine", "nowhere"]);
    assert_ne!(failed.status.code(), Some(0));
    assert!(
        stdout(&failed).contains("no machine called nowhere"),
        "{}",
        stdout(&failed)
    );
}
