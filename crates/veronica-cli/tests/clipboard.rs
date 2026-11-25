mod common;

use std::io::Write;
use std::process::{Command, Output, Stdio};

use serde_json::{json, Value};
use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_core::settings::Settings;
use veronica_host::{HostIdentity, Installer, Layout};

struct Home {
    root: TempDir,
}

impl Home {
    fn with_clipboard() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&home.directories());
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "clipboard"))
            .unwrap();
        home
    }

    fn directories(&self) -> AppDirectories {
        AppDirectories::with_env(self.root.path(), EnvOverrides::default())
    }

    fn vr(&self, args: &[&str], stdin: Option<&str>) -> Output {
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
        let mut child = command
            .env("HOME", self.root.path())
            .env("NO_COLOR", "1")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(text) = stdin {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(text.as_bytes())
                .unwrap();
        }
        drop(child.stdin.take());
        child.wait_with_output().unwrap()
    }
}

fn out(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn err(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn installing_the_extension_switches_on_clipboard_capture() {
    let home = Home::with_clipboard();
    let settings = Settings::load(&home.directories().settings_file()).unwrap();
    assert_eq!(settings.get("clipboardEnabled"), Some(&Value::Bool(true)));
}

#[test]
fn copies_arrive_on_stdin_and_come_back_byte_for_byte() {
    let home = Home::with_clipboard();
    let recorded = home.vr(&["clipboard", "record"], Some("hello\nworld\n"));
    assert_eq!(recorded.status.code(), Some(0), "{}", err(&recorded));
    assert_eq!(out(&recorded), "");

    let listed: Value =
        serde_json::from_slice(&home.vr(&["--json", "clip", "ls"], None).stdout).unwrap();
    assert_eq!(listed[0]["preview"], "hello world");
    assert_eq!(listed[0]["lines"], 2);

    assert_eq!(
        out(&home.vr(&["clipboard", "get", "1"], None)),
        "hello\nworld"
    );
}

#[test]
fn forgetting_asks_first_exactly_as_before() {
    let home = Home::with_clipboard();
    home.vr(&["clipboard", "record"], Some("secret note"));

    let refused = home.vr(&["clipboard", "clear"], None);
    assert_eq!(refused.status.code(), Some(5));
    assert_eq!(
        out(&refused),
        "`vr clipboard clear` removes or replaces data.\n\nNothing was changed. To go ahead, run:\n  vr clipboard clear --yes\n"
    );
    assert_eq!(err(&refused), "vr: confirmation required\n");

    let previewed: Value = serde_json::from_slice(
        &home
            .vr(&["--json", "clipboard", "remove", "1"], None)
            .stdout,
    )
    .unwrap();
    assert_eq!(
        previewed,
        json!({
            "confirmationRequired": true,
            "preview": { "command": "clipboard remove", "effect": "destructive" },
            "rerun": "vr --json clipboard remove 1 --yes",
        })
    );

    let removed = home.vr(&["clipboard", "remove", "1", "--yes"], None);
    assert_eq!(out(&removed), "removed 1\n");
    let missing = home.vr(&["clipboard", "get", "1"], None);
    assert_eq!(missing.status.code(), Some(1));
    assert_eq!(err(&missing), "vr: no clipboard entry 1\n");
}
