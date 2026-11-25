mod common;

use std::process::{Command, Output, Stdio};

use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

struct Home {
    root: TempDir,
}

impl Home {
    fn with_emoji() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&AppDirectories::with_env(
            home.root.path(),
            EnvOverrides::default(),
        ));
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "emoji"))
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
fn the_emoji_commands_come_from_the_installed_extension() {
    let home = Home::with_emoji();
    let help = home.vr(&["emoji", "--help"]);
    assert_eq!(help.status.code(), Some(0));
    let text = stdout(&help);
    assert!(
        text.contains("Search the emoji catalogue and copy what you pick"),
        "{text}"
    );
    for leaf in ["list", "groups", "copy", "recents", "forget"] {
        assert!(text.contains(leaf), "{text}");
    }
    let copy = stdout(&home.vr(&["emoji", "copy", "--help"]));
    assert!(
        copy.contains("Usage: vr emoji copy [OPTIONS] <QUERY>"),
        "{copy}"
    );
}

#[test]
fn the_catalogue_is_searched_through_the_backend() {
    let home = Home::with_emoji();
    let found = document(&home.vr(&["--json", "emoji", "search", "grinning", "--limit", "3"]));
    let rows = found.as_array().unwrap();
    assert!(!rows.is_empty() && rows.len() <= 3, "{found}");
    assert!(
        rows[0]["name"].as_str().unwrap().contains("grinning"),
        "{found}"
    );
    let groups = document(&home.vr(&["emoji", "groups", "--json"]));
    assert!(groups.as_array().unwrap().len() > 5, "{groups}");
    let recents = document(&home.vr(&["--json", "emoji", "recents"]));
    assert_eq!(recents, serde_json::json!([]));
    let recorded = document(&home.vr(&["--json", "emoji", "copy", "grinning", "--no-copy"]));
    assert!(recorded["copiedVia"].is_null(), "{recorded}");
    let recents = document(&home.vr(&["--json", "emoji", "recents"]));
    assert_eq!(recents[0]["count"], 1, "{recents}");
}
