mod common;

use std::net::TcpListener;
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
fn real_listener_can_be_found_but_not_stopped_without_confirmation_or_matching_identity() {
    let home = Home::with_system();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let output = home.vr(&["--json", "ports", "list", "--port", &port]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = report["ports"].as_array().unwrap();
    assert!(!rows.is_empty());
    assert!(rows
        .iter()
        .all(|row| row["port"].as_u64().unwrap().to_string() == port));
    let row = rows.iter().find(|row| row["protocol"] == "TCP").unwrap();
    let owner = row["owners"]
        .as_array()
        .unwrap()
        .iter()
        .find(|owner| owner["pid"].as_u64() == Some(u64::from(std::process::id())))
        .unwrap();
    let pid = std::process::id().to_string();
    let inode = row["inode"].as_str().unwrap();
    let refused = home.vr(&[
        "--json",
        "ports",
        "stop",
        "--pid",
        &pid,
        "--started-at",
        owner["startedAt"].as_str().unwrap(),
        "--inode",
        inode,
    ]);
    assert_eq!(refused.status.code(), Some(5));
    assert!(out(&refused).contains("--yes"));
    let stale = home.vr(&[
        "--json",
        "ports",
        "stop",
        "--pid",
        &pid,
        "--started-at",
        "stale",
        "--inode",
        inode,
        "--yes",
    ]);
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("process changed"));
    assert!(TcpListener::bind(listener.local_addr().unwrap()).is_err());
}

#[test]
fn docker_mutations_require_confirmation_before_contacting_the_engine() {
    let home = Home::with_system();
    let id = "a".repeat(64);
    let output = home.vr(&[
        "--json",
        "docker",
        "action",
        "stop",
        &id,
        "--endpoint",
        "unix:///var/run/docker.sock",
    ]);
    assert_eq!(output.status.code(), Some(5));
    assert!(out(&output).contains("--yes"));
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
