mod common;

use std::process::{Command, Output, Stdio};

use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

struct Home {
    root: TempDir,
}

impl Home {
    fn with_workspace() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&AppDirectories::with_env(
            home.root.path(),
            EnvOverrides::default(),
        ));
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "workspace"))
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
fn the_workspace_commands_come_from_the_installed_extension() {
    let home = Home::with_workspace();
    let help = stdout(&home.vr(&["workspace", "--help"]));
    assert!(
        help.contains("On-demand, read-only developer project checks"),
        "{help}"
    );
    let graft = stdout(&home.vr(&["workspace", "graft", "--help"]));
    assert!(graft.contains("discover"), "{graft}");
    assert!(graft.contains("call"), "{graft}");
    let doctor = stdout(&home.vr(&["workspace", "doctor", "--help"]));
    assert!(doctor.contains("--root <ROOT>"), "{doctor}");
}

#[test]
fn the_doctor_inspects_a_project_through_the_backend() {
    let home = Home::with_workspace();
    let project = home.root.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("package.json"), "{}").unwrap();
    let report = document(&home.vr(&[
        "--json",
        "workspace",
        "doctor",
        "--root",
        project.to_str().unwrap(),
    ]));
    assert!(report["checks"].as_array().is_some(), "{report}");
}
