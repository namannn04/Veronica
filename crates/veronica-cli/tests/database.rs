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
    fn with_database() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&home.directories());
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "database"))
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
            .current_dir(self.root.path())
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

fn sqlite(home: &Home) {
    let path = home.root.path().join("t.db");
    std::fs::write(&path, b"").unwrap();
    let created = Command::new("python3")
        .args([
            "-c",
            "import sqlite3,sys;c=sqlite3.connect(sys.argv[1]);c.execute('create table items(id integer primary key, name text)');c.executemany('insert into items(name) values(?)',[('a',),('b',)]);c.commit()",
        ])
        .arg(&path)
        .status()
        .unwrap();
    assert!(created.success());
}

#[test]
fn a_sqlite_connection_is_saved_read_and_queried_through_the_extension() {
    let home = Home::with_database();
    sqlite(&home);
    assert_eq!(
        out(&home.vr(
            &["database", "connections", "add", "local", "sqlite", "t.db"],
            None
        )),
        "saved local (t.db)\ncredentials go to the desktop keyring\n"
    );
    let page: Value = serde_json::from_slice(
        &home
            .vr(
                &[
                    "--json",
                    "db",
                    "read",
                    "local",
                    "main.items",
                    "--limit",
                    "5",
                ],
                None,
            )
            .stdout,
    )
    .unwrap();
    assert_eq!(page["rows"].as_array().unwrap().len(), 2);
    let refused = home.vr(&["database", "query", "local", "delete from items"], None);
    assert_eq!(refused.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("only runs statements that read"));
}

#[test]
fn saved_queries_and_passwords_arrive_on_stdin_only_when_asked_for() {
    let home = Home::with_database();
    sqlite(&home);
    home.vr(
        &["database", "connections", "add", "local", "sqlite", "t.db"],
        None,
    );
    home.vr(
        &[
            "database",
            "saved-queries",
            "save",
            "given",
            "select 1",
            "--connection",
            "local",
        ],
        Some("ignored"),
    );
    home.vr(
        &[
            "database",
            "saved-queries",
            "save",
            "piped",
            "--connection",
            "local",
        ],
        Some("select 2\n"),
    );
    let saved: Value = serde_json::from_slice(
        &home
            .vr(&["--json", "database", "saved-queries", "list"], None)
            .stdout,
    )
    .unwrap();
    let texts: Vec<&str> = saved
        .as_array()
        .unwrap()
        .iter()
        .map(|query| query["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts, ["select 1", "select 2"]);

    let added = home.vr(
        &[
            "database",
            "connections",
            "add",
            "pg",
            "postgresql",
            "127.0.0.1:1",
            "--username",
            "me",
            "--password-stdin",
        ],
        Some("secret\n"),
    );
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let pg: Value = serde_json::from_slice(
        &home
            .vr(&["--json", "database", "connections", "get", "pg"], None)
            .stdout,
    )
    .unwrap();
    assert_eq!(pg["authentication"]["source"], "stdin");
}
