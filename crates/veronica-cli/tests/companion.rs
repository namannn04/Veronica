mod common;

use std::io::Write;
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

struct Home {
    root: TempDir,
}

impl Home {
    fn with_companion() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&home.directories());
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "companion"))
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

#[test]
fn notes_are_written_listed_pinned_and_removed_through_the_extension() {
    let home = Home::with_companion();
    assert_eq!(
        out(&home.vr(&["companion", "list"], None)),
        "nothing saved yet\n"
    );
    assert_eq!(
        out(&home.vr(
            &["companion", "add", "First", "hello body"],
            Some("ignored")
        )),
        "saved note 1: First\n"
    );
    assert_eq!(
        out(&home.vr(&["companion", "add", "Second"], Some("piped\n"))),
        "saved note 2: Second\n"
    );
    let second: Value =
        serde_json::from_slice(&home.vr(&["--json", "companion", "show", "2"], None).stdout)
            .unwrap();
    assert_eq!(second["body"], "piped");
    let first: Value =
        serde_json::from_slice(&home.vr(&["--json", "companion", "show", "1"], None).stdout)
            .unwrap();
    assert_eq!(first["body"], "hello body");
    assert_eq!(
        out(&home.vr(&["companion", "pin", "1"], None)),
        "pinned First\n"
    );

    let refused = home.vr(&["companion", "remove", "2"], None);
    assert_eq!(refused.status.code(), Some(5));
    assert!(out(&refused).starts_with("`vr companion remove` removes or replaces data."));
    assert_eq!(
        out(&home.vr(&["companion", "rm", "2", "--yes"], None)),
        "removed 2\n"
    );
    let listed: Value =
        serde_json::from_slice(&home.vr(&["--json", "companion", "ls"], None).stdout).unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["pinned"], true);
}

#[test]
fn an_unknown_kind_is_refused() {
    let home = Home::with_companion();
    let refused = home.vr(&["companion", "list", "--kind", "bad"], None);
    assert_eq!(refused.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("unknown kind 'bad'"));
}
