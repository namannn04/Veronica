mod common;

use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

struct Home {
    root: TempDir,
}

impl Home {
    fn with_system() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&AppDirectories::with_env(
            home.root.path(),
            EnvOverrides::default(),
        ));
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "system"))
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
fn system_readings_come_from_the_extension() {
    let home = Home::with_system();
    let snapshot: Value =
        serde_json::from_slice(&home.vr(&["--json", "system", "snapshot"]).stdout).unwrap();
    assert!(snapshot["memory"]["totalBytes"].as_u64().unwrap() > 0);
    assert!(out(&home.vr(&["system", "disks"])).starts_with("VOLUME"));
    let ports: Value =
        serde_json::from_slice(&home.vr(&["--json", "ports", "list"]).stdout).unwrap();
    assert!(ports["ports"].is_array());
    let docker: Value =
        serde_json::from_slice(&home.vr(&["--json", "docker", "status"]).stdout).unwrap();
    assert!(docker["status"].is_string());
}

#[test]
fn stopping_a_port_owner_waits_for_confirmation() {
    let home = Home::with_system();
    let refused = home.vr(&[
        "ports",
        "stop",
        "--pid",
        "1",
        "--started-at",
        "x",
        "--inode",
        "1",
    ]);
    assert_eq!(refused.status.code(), Some(5));
    assert!(out(&refused).starts_with("`vr ports stop` removes or replaces data."));
    let remote = home.vr(&[
        "docker",
        "action",
        "start",
        "abc",
        "--endpoint",
        "tcp://127.0.0.1:1",
        "--yes",
    ]);
    assert_eq!(remote.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&remote.stderr).contains("Only a local Docker endpoint"));
}
