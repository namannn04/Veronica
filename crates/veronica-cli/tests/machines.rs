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
    fn with_machines() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&AppDirectories::with_env(
            home.root.path(),
            EnvOverrides::default(),
        ));
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "machines"))
            .unwrap();
        fs::create_dir_all(home.root.path().join(".ssh")).unwrap();
        fs::write(
            home.root.path().join(".ssh/config"),
            "Host box\n  HostName 10.0.0.9\nHost other\n",
        )
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

fn out(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn hosts_are_discovered_added_and_removed_through_the_extension() {
    let home = Home::with_machines();
    assert_eq!(
        out(&home.vr(&["machines", "discover"])),
        "SSH HOST\nbox\nother\n\nAdd one with: vr machines add <host>\n"
    );
    assert_eq!(
        out(&home.vr(&["machines", "add", "box", "--name", "Box"])),
        "added box -> ssh box\n"
    );
    let listed: Value =
        serde_json::from_slice(&home.vr(&["--json", "machine", "ls", "--offline"]).stdout).unwrap();
    assert_eq!(listed[1]["id"], "box");

    let refused = home.vr(&["machines", "remove", "box"]);
    assert_eq!(refused.status.code(), Some(5));
    assert!(out(&refused).starts_with("`vr machines remove` removes or replaces data."));
    assert_eq!(
        out(&home.vr(&["machines", "rm", "box", "--yes"])),
        "removed box\n"
    );
}

#[test]
fn this_computer_refuses_remote_only_actions() {
    let home = Home::with_machines();
    let power = home.vr(&["machines", "power", "local", "restart", "--yes"]);
    assert_eq!(power.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&power.stderr).contains("will not restart the computer"));
}
