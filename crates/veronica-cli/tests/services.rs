use serde_json::Value;
use std::{
    net::TcpListener,
    path::Path,
    process::{Command, Output},
};

fn vr(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vr"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_RUNTIME_DIR", root.join("runtime"))
        .arg("--json")
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn real_listener_can_be_found_but_not_stopped_without_confirmation_or_matching_identity() {
    let temp = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let output = vr(temp.path(), &["ports", "list", "--port", &port]);
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
    let args = [
        "ports",
        "stop",
        "--pid",
        &pid,
        "--started-at",
        owner["startedAt"].as_str().unwrap(),
        "--inode",
        row["inode"].as_str().unwrap(),
    ];
    let refused = vr(temp.path(), &args);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--confirm"));
    let stale = vr(
        temp.path(),
        &[
            "ports",
            "stop",
            "--pid",
            &pid,
            "--started-at",
            "stale",
            "--inode",
            row["inode"].as_str().unwrap(),
            "--confirm",
        ],
    );
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("process changed"));
    assert!(TcpListener::bind(listener.local_addr().unwrap()).is_err());
}
#[test]
fn docker_mutations_require_confirmation_before_contacting_the_engine() {
    let temp = tempfile::tempdir().unwrap();
    let output = vr(
        temp.path(),
        &[
            "docker",
            "action",
            "stop",
            &"a".repeat(64),
            "--endpoint",
            "unix:///var/run/docker.sock",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--confirm"));
}
