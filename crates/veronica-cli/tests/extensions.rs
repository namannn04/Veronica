use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

use serde_json::{json, Value};
use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::trust::DEVELOPMENT_KEY;
use veronica_host::{HostIdentity, Installer, Layout};
use veronica_protocol::canonical;
use veronica_protocol::catalog::Catalog;

const XDG: [&str; 5] = [
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_STATE_HOME",
    "XDG_RUNTIME_DIR",
];

fn fixture_backend() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let output = Command::new(cargo)
            .args([
                "build",
                "--quiet",
                "--package",
                "veronica-ext-fixture",
                "--message-format=json",
            ])
            .stderr(Stdio::inherit())
            .output()
            .expect("cargo runs");
        assert!(output.status.success(), "building the fixture failed");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|message| message["target"]["name"] == "veronica-ext-fixture")
            .find_map(|message| message["executable"].as_str().map(PathBuf::from))
            .expect("cargo reports the fixture executable")
    })
}

struct Home {
    root: TempDir,
}

impl Home {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
        }
    }

    fn with_fixture() -> Self {
        let home = Self::new();
        let source = home.root.path().join("fixture-source");
        fs::create_dir_all(source.join("bin")).unwrap();
        let manifest =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions/fixture/manifest.json");
        fs::copy(manifest, source.join("manifest.json")).unwrap();
        let backend = source.join("bin/backend");
        fs::copy(fixture_backend(), &backend).unwrap();
        fs::set_permissions(&backend, fs::Permissions::from_mode(0o755)).unwrap();
        Installer::new(home.layout(), HostIdentity::current())
            .install_sideload(&source)
            .unwrap();
        home
    }

    fn layout(&self) -> Layout {
        Layout::new(&AppDirectories::with_env(
            self.root.path(),
            EnvOverrides::default(),
        ))
    }

    fn vr(&self, args: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_vr"));
        for key in XDG {
            command.env_remove(key);
        }
        command
            .env("HOME", self.root.path())
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("vr exits normally")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn document(output: &Output) -> Value {
    let text = stdout(output);
    let value: Value = serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{error}: {text:?} {}", stderr(output)));
    assert_eq!(text, canonical::value_to_string_pretty(&value), "canonical");
    value
}

#[test]
fn help_shows_the_extension_tree_without_starting_it() {
    let home = Home::with_fixture();
    let top = home.vr(&["--help"]);
    assert_eq!(code(&top), 0);
    assert!(stdout(&top).contains("fixture"), "{}", stdout(&top));
    let root = home.vr(&["fixture", "--help"]);
    assert_eq!(code(&root), 0);
    for name in ["echo", "fail", "wipe", "wait", "Pretend to delete one item"] {
        assert!(stdout(&root).contains(name), "{}", stdout(&root));
    }
    let leaf = home.vr(&["fixture", "fail", "--help"]);
    assert_eq!(code(&leaf), 0);
    assert!(stdout(&leaf).contains("--code <code>"), "{}", stdout(&leaf));
    assert!(!home.layout().log_file("fixture").exists());
}

#[test]
fn extension_commands_run_through_the_backend() {
    let home = Home::with_fixture();
    let echoed = home.vr(&["--json", "fixture", "echo", "hello"]);
    assert_eq!(code(&echoed), 0, "{}", stderr(&echoed));
    let echoed = document(&echoed);
    assert_eq!(echoed["text"], "hello");
    assert_eq!(echoed["mode"], "oneshot");
    let plain = home.vr(&["fixture", "echo", "plain words"]);
    assert_eq!(code(&plain), 0);
    assert_eq!(stdout(&plain), "plain words\n");
    let after = home.vr(&["fixture", "echo", "--json", "late"]);
    assert_eq!(document(&after)["text"], "late");
}

#[test]
fn exit_codes_follow_the_table() {
    let home = Home::with_fixture();
    let denied = home.vr(&["--json", "fixture", "fail", "--code", "6"]);
    assert_eq!(code(&denied), 6);
    assert_eq!(document(&denied)["error"]["code"], 6);
    let plain = home.vr(&["fixture", "fail", "--code", "3"]);
    assert_eq!(code(&plain), 3);
    assert!(stderr(&plain).contains("failed with 3"));
    assert_eq!(code(&home.vr(&["fixture", "echo"])), 2);
    assert_eq!(code(&home.vr(&["fixture", "fail", "--code", "x"])), 2);
    let slow = home.vr(&["--json", "fixture", "wait", "--seconds", "5"]);
    assert_eq!(code(&slow), 10);
    assert_eq!(document(&slow)["error"]["code"], 10);
    let quick = home.vr(&["--json", "fixture", "wait", "--seconds", "0"]);
    assert_eq!(document(&quick), json!({ "waited": 0 }));
}

#[test]
fn destructive_commands_preview_until_confirmed() {
    let home = Home::with_fixture();
    let preview = home.vr(&["--json", "fixture", "wipe", "old files"]);
    assert_eq!(code(&preview), 5);
    assert_eq!(
        document(&preview),
        json!({
            "confirmationRequired": true,
            "preview": { "wouldRemove": "old files" },
            "rerun": "vr --json fixture wipe 'old files' --yes"
        })
    );
    let text = home.vr(&["fixture", "wipe", "cache"]);
    assert_eq!(code(&text), 5);
    assert!(stdout(&text).contains("would remove cache"));
    assert!(stdout(&text).contains("vr fixture wipe cache --yes"));
    let confirmed = home.vr(&["--json", "fixture", "wipe", "cache", "--yes"]);
    assert_eq!(code(&confirmed), 0);
    assert_eq!(document(&confirmed), json!({ "removed": "cache" }));
    let short = home.vr(&["-y", "fixture", "wipe", "cache"]);
    assert_eq!(code(&short), 0);
    assert_eq!(stdout(&short), "removed cache\n");
}

#[test]
fn invoke_calls_one_backend_method() {
    let home = Home::with_fixture();
    let mode = home.vr(&["--json", "invoke", "fixture", r#"{"method": "mode"}"#]);
    assert_eq!(code(&mode), 0, "{}", stderr(&mode));
    assert_eq!(document(&mode)["mode"], "oneshot");
    let unknown = home.vr(&[
        "--json",
        "invoke",
        "fixture",
        r#"{"method": "nope", "params": {}}"#,
    ]);
    assert_eq!(code(&unknown), 2);
    assert_eq!(document(&unknown)["error"]["code"], 2);
    assert_eq!(code(&home.vr(&["invoke", "fixture", "not json"])), 2);
    let crashed = home.vr(&["invoke", "fixture", r#"{"method": "crash"}"#]);
    assert_eq!(code(&crashed), 9, "{}", stderr(&crashed));
    let missing = home.vr(&["--json", "invoke", "lantern", r#"{"method": "mode"}"#]);
    assert_eq!(code(&missing), 4);
    let message = document(&missing)["error"]["message"].clone();
    assert!(message
        .as_str()
        .unwrap()
        .contains("vr extensions install lantern"));
}

fn write_catalog(layout: &Layout) {
    let catalog: Catalog = serde_json::from_value(json!({
        "schema": 1,
        "revision": 9_000_000_000_u64,
        "generatedAt": "2026-11-24T00:00:00Z",
        "expiresAt": "2099-01-01T00:00:00Z",
        "source": { "repository": "example/extensions", "commit": "0123abcd" },
        "categories": [],
        "profiles": [],
        "extensions": [{
            "id": "lantern",
            "title": "Lantern",
            "summary": "A lamp.",
            "category": "labs",
            "latest": "1.0.0",
            "versions": []
        }]
    }))
    .unwrap();
    let key_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../veronica-host/tests/fixtures/development-catalog.key");
    let key = veronica_pack::signing::load_private(key_path.to_str().unwrap()).unwrap();
    let envelope = veronica_pack::signing::sign(
        &catalog.to_payload_bytes().unwrap(),
        &key,
        DEVELOPMENT_KEY.id,
    )
    .unwrap();
    let path = layout.catalog_file();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, envelope.to_canonical_string().unwrap()).unwrap();
}

#[test]
fn missing_roots_point_at_the_extension_to_install() {
    let home = Home::new();
    write_catalog(&home.layout());
    let text = home.vr(&["lantern", "on"]);
    assert_eq!(code(&text), 4);
    assert!(
        stderr(&text).contains("install it with `vr extensions install lantern`"),
        "{}",
        stderr(&text)
    );
    let json = home.vr(&["--json", "lantern"]);
    assert_eq!(code(&json), 4);
    assert_eq!(document(&json)["error"]["code"], 4);
    let nonsense = home.vr(&["--json", "nonsense"]);
    assert_eq!(code(&nonsense), 2);
    assert_eq!(document(&nonsense)["error"]["code"], 2);
}

#[test]
fn disabled_extensions_are_not_routed() {
    let home = Home::with_fixture();
    let installed = home.layout().installed_file();
    let mut registry: Value =
        serde_json::from_str(&fs::read_to_string(&installed).unwrap()).unwrap();
    registry["extensions"]["fixture"]["enabled"] = json!(false);
    fs::write(&installed, registry.to_string()).unwrap();
    let output = home.vr(&["fixture", "echo", "hi"]);
    assert_eq!(code(&output), 4);
    assert!(
        stderr(&output).contains("fixture is disabled"),
        "{}",
        stderr(&output)
    );
    assert!(!stdout(&home.vr(&["--help"])).contains("fixture"));
}

#[test]
fn host_commands_print_canonical_json() {
    let home = Home::new();
    for (key, value) in [("zeta", "1"), ("alpha", "true"), ("middle", "text")] {
        let set = home.vr(&["--json", "config", "set", key, value]);
        assert_eq!(code(&set), 0, "{}", stderr(&set));
        document(&set);
    }
    let listed = document(&home.vr(&["--json", "config", "list"]));
    assert_eq!(listed["alpha"], true);
    let diagnosed = home.vr(&["--json", "diagnose"]);
    assert_eq!(code(&diagnosed), 0, "{}", stderr(&diagnosed));
    document(&diagnosed);
    let missing = home.vr(&["--json", "config", "get", "absent"]);
    assert_eq!(code(&missing), 1);
    assert_eq!(document(&missing)["error"]["code"], 1);
}

fn subcommands(help: &str) -> Vec<String> {
    help.lines()
        .skip_while(|line| line.trim() != "Commands:")
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .map(|name| name.trim_end_matches(',').to_string())
        .filter(|name| name != "help")
        .collect()
}

#[test]
fn every_host_command_still_prints_help() {
    let home = Home::new();
    let mut pending: Vec<Vec<String>> = vec![Vec::new()];
    let mut visited = 0;
    while let Some(path) = pending.pop() {
        let mut args: Vec<&str> = path.iter().map(String::as_str).collect();
        args.push("--help");
        let output = home.vr(&args);
        assert_eq!(
            code(&output),
            0,
            "vr {}: {}",
            args.join(" "),
            stderr(&output)
        );
        let help = stdout(&output);
        assert!(help.contains("Usage:"), "vr {}", args.join(" "));
        visited += 1;
        for name in subcommands(&help) {
            let mut child = path.clone();
            child.push(name);
            pending.push(child);
        }
    }
    assert!(visited > 100, "walked only {visited} commands");
    let extensions = stdout(&home.vr(&["extensions", "--help"]));
    assert!(extensions.contains("enable"));
    let graft = stdout(&home.vr(&["workspace", "graft", "--help"]));
    assert!(graft.contains("discover"));
}
