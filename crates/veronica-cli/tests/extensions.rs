mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{json, Value};
use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::trust::DEVELOPMENT_KEY;
use veronica_host::{HostIdentity, Installer, Layout};
use veronica_pack::bundle::bundle;
use veronica_pack::fingerprint::Stamp;
use veronica_pack::pack::{pack, PackInputs};
use veronica_protocol::canonical;
use veronica_protocol::catalog::Catalog;
use veronica_protocol::manifest::Target;
const XDG: [&str; 5] = [
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_STATE_HOME",
    "XDG_RUNTIME_DIR",
];

struct Home {
    root: TempDir,
}

impl Home {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
        }
    }

    fn source(&self) -> PathBuf {
        common::fixture_source(self.root.path())
    }

    fn with_fixture() -> Self {
        let home = Self::new();
        Installer::new(home.layout(), HostIdentity::current())
            .install_sideload(&home.source())
            .unwrap();
        home
    }

    fn settings(&self) -> Value {
        let path = self.root.path().join(".config/veronica/settings.json");
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    fn layout(&self) -> Layout {
        Layout::new(&AppDirectories::with_env(
            self.root.path(),
            EnvOverrides::default(),
        ))
    }

    fn vr(&self, args: &[&str]) -> Output {
        self.vr_with(args, &[])
    }

    fn vr_with(&self, args: &[&str], env: &[(&str, &Path)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_vr"));
        for key in XDG {
            command.env_remove(key);
        }
        for (key, value) in env {
            command.env(key, value);
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
    let literal = home.vr(&["--json", "fixture", "wipe", "--", "--yes"]);
    assert_eq!(code(&literal), 5);
    assert_eq!(
        document(&literal),
        json!({
            "confirmationRequired": true,
            "preview": { "wouldRemove": "--yes" },
            "rerun": "vr --json fixture wipe --yes -- --yes"
        })
    );
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
    let surfaces = home.vr(&["--json", "surfaces"]);
    assert_eq!(code(&surfaces), 0, "{}", stderr(&surfaces));
    assert_eq!(document(&surfaces), json!([]));
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

fn ids(listing: &Value) -> Vec<String> {
    listing
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn sideloads_install_and_remove_only_when_confirmed() {
    let home = Home::new();
    let source = home.source();
    let source = source.to_str().unwrap();
    let preview = home.vr(&["--json", "extensions", "install", source]);
    assert_eq!(code(&preview), 5, "{}", stderr(&preview));
    let preview = document(&preview);
    assert_eq!(preview["confirmationRequired"], true);
    assert_eq!(preview["preview"]["id"], "fixture");
    assert_eq!(preview["preview"]["origin"], "sideload");
    assert!(!home.layout().current_link("fixture").exists());

    let installed = home.vr(&["--json", "--yes", "extensions", "install", source]);
    assert_eq!(code(&installed), 0, "{}", stderr(&installed));
    assert_eq!(document(&installed)["origin"], "sideload");
    let listed = document(&home.vr(&["--json", "extensions", "ls"]));
    assert!(ids(&listed).contains(&"fixture".to_string()));
    assert!(ids(&listed).contains(&"herdr".to_string()));
    assert_eq!(code(&home.vr(&["fixture", "echo", "hi"])), 0);

    let kept = home.vr(&["--json", "extensions", "remove", "fixture", "--purge"]);
    assert_eq!(code(&kept), 5);
    assert_eq!(document(&kept)["preview"]["purge"], true);
    assert!(home.layout().current_link("fixture").exists());
    let removed = home.vr(&["--json", "extensions", "remove", "fixture", "--yes"]);
    assert_eq!(code(&removed), 0, "{}", stderr(&removed));
    assert_eq!(
        document(&removed),
        json!({ "removed": "fixture", "purged": false })
    );
    let listed = document(&home.vr(&["--json", "extensions", "ls"]));
    assert!(!ids(&listed).contains(&"fixture".to_string()));
    let builtin = home.vr(&["--json", "-y", "extensions", "remove", "herdr"]);
    assert_eq!(code(&builtin), 2);
}

#[test]
fn enable_and_disable_reach_installed_and_built_in_extensions() {
    let home = Home::with_fixture();
    let off = home.vr(&["--json", "extensions", "disable", "fixture"]);
    assert_eq!(code(&off), 0, "{}", stderr(&off));
    assert_eq!(
        document(&off),
        json!({ "id": "fixture", "origin": "sideload", "enabled": false })
    );
    assert_eq!(code(&home.vr(&["fixture", "echo", "hi"])), 4);
    assert_eq!(code(&home.vr(&["extensions", "enable", "fixture"])), 0);
    assert_eq!(code(&home.vr(&["fixture", "echo", "hi"])), 0);

    let builtin = home.vr(&["--json", "extensions", "enable", "herdr"]);
    assert_eq!(code(&builtin), 0, "{}", stderr(&builtin));
    assert_eq!(document(&builtin)["origin"], "builtin");
    let key = veronica_core::extensions::entry("herdr")
        .unwrap()
        .defaults_key;
    assert_eq!(home.settings()[key], true);
    let missing = home.vr(&["--json", "extensions", "enable", "lantern"]);
    assert_eq!(code(&missing), 3);
    assert_eq!(document(&missing)["error"]["code"], 3);
}

#[test]
fn verify_rehashes_installed_trees() {
    let home = Home::with_fixture();
    let clean = home.vr(&["--json", "extensions", "verify"]);
    assert_eq!(code(&clean), 0, "{}", stderr(&clean));
    assert_eq!(document(&clean)[0]["ok"], true);
    let backend = fs::canonicalize(home.layout().current_link("fixture"))
        .unwrap()
        .join("bin/backend");
    let mut bytes = fs::read(&backend).unwrap();
    bytes.push(0);
    fs::write(&backend, bytes).unwrap();
    let tampered = home.vr(&["--json", "extensions", "verify", "fixture"]);
    assert_eq!(code(&tampered), 8);
    let tampered = document(&tampered);
    assert_eq!(tampered[0]["ok"], false);
    assert!(tampered[0]["problem"]
        .as_str()
        .unwrap()
        .contains("bin/backend"));
    assert_eq!(code(&home.vr(&["extensions", "verify", "herdr"])), 2);
}

#[test]
fn doctor_checks_backends_and_the_catalog() {
    let home = Home::with_fixture();
    let report = home.vr(&["--json", "extensions", "doctor"]);
    assert_eq!(code(&report), 0, "{}", stderr(&report));
    let checks = document(&report);
    let level = |name: &str| {
        checks
            .as_array()
            .unwrap()
            .iter()
            .find(|check| check["name"] == name)
            .map(|check| check["level"].clone())
    };
    assert_eq!(level("exec permission"), Some(json!("ok")));
    assert_eq!(level("fixture: backend"), Some(json!("ok")));
    assert_eq!(level("catalog"), Some(json!("warn")));
    assert_eq!(level("leftovers"), Some(json!("ok")));
}

#[test]
fn auto_update_preferences_use_the_shared_settings() {
    let home = Home::new();
    let shown = document(&home.vr(&["--json", "extensions", "auto-update"]));
    assert_eq!(
        shown,
        json!({ "enabled": true, "intervalHours": 6, "excluded": [] })
    );
    for change in [&["off"][..], &["hours", "3"], &["exclude", "media"]] {
        let mut args = vec!["--json", "extensions", "auto-update"];
        args.extend_from_slice(change);
        assert_eq!(code(&home.vr(&args)), 0);
    }
    let settings = home.settings();
    assert_eq!(settings["extensionsAutoUpdate"], false);
    assert_eq!(settings["extensionsUpdateHours"], 3);
    assert_eq!(settings["extensionsAutoUpdateOff"], json!(["media"]));
    assert_eq!(
        code(&home.vr(&["extensions", "auto-update", "hours", "0"])),
        2
    );
    let setup = home.vr(&["--json", "extensions", "setup", "--profile", "developer"]);
    assert_eq!(code(&setup), 0, "{}", stderr(&setup));
    assert_eq!(document(&setup)["profile"], "developer");
}

#[test]
fn catalog_commands_work_offline_from_the_cached_catalog() {
    let home = Home::with_fixture();
    write_catalog(&home.layout());
    let found = document(&home.vr(&["--json", "extensions", "search", "LAMP"]));
    assert_eq!(ids(&found), ["lantern"]);
    let status = document(&home.vr(&["--json", "extensions", "status"]));
    assert_eq!(status["catalog"]["source"], "cache");
    assert_eq!(status["extensions"][0]["id"], "fixture");
    let info = document(&home.vr(&["--json", "extensions", "info", "fixture"]));
    assert_eq!(info["installed"]["origin"], "sideload");
    assert!(info["commands"]
        .as_array()
        .unwrap()
        .contains(&json!("fixture wipe")));
    let listed = document(&home.vr(&["--json", "extensions", "info", "lantern"]));
    assert_eq!(listed["latest"], "1.0.0");
    assert!(listed.get("installed").is_none());
    assert_eq!(code(&home.vr(&["extensions", "info", "nothing"])), 3);
    assert_eq!(code(&home.vr(&["extensions", "install", "nothing"])), 3);
    assert_eq!(code(&home.vr(&["extensions", "install", "lantern"])), 4);
    let updates = document(&home.vr(&["--json", "extensions", "update"]));
    assert_eq!(updates, json!({ "updated": [] }));
    assert_eq!(code(&home.vr(&["extensions", "update", "fixture"])), 1);
    assert_eq!(code(&home.vr(&["extensions", "setup", "--dry-run"])), 3);
}

fn bundled_snap(root: &Path) -> PathBuf {
    let repo = root.join("repo");
    let manifest = json!({
        "schema": 1, "id": "probe", "version": "1.0", "title": "Probe",
        "summary": "A bundled test extension.", "category": "labs",
        "engines": { "host": ">=0.1.0", "hostApi": 1 },
        "targets": ["linux-x86_64"], "profiles": ["simple"]
    });
    fs::create_dir_all(repo.join("extensions/probe")).unwrap();
    fs::write(
        repo.join("extensions/probe/manifest.json"),
        manifest.to_string(),
    )
    .unwrap();
    fs::write(repo.join("LICENSE"), "license\n").unwrap();
    fs::write(repo.join("NOTICE"), "notice\n").unwrap();
    let stamp = Stamp {
        id: "probe".to_string(),
        version: "1.0.2".parse().unwrap(),
        fingerprint: "sha256:probe".to_string(),
        commit: "0123abcd".to_string(),
        published_at: "2026-11-25T00:00:00Z".to_string(),
        inputs: Vec::new(),
    };
    pack(
        &PackInputs {
            root: &repo,
            backend: None,
            ui: None,
            out: &repo.join("out"),
            target: Target::LinuxX86_64,
        },
        &stamp,
    )
    .unwrap();
    let snap = root.join("snap");
    bundle(
        &repo.join("out"),
        &snap.join("usr/share/veronica/extensions"),
    )
    .unwrap();
    snap
}

#[test]
fn bundled_packages_install_offline_with_the_bundled_origin() {
    let home = Home::new();
    let packages = tempfile::tempdir().unwrap();
    let snap = bundled_snap(packages.path());
    let env = [
        ("SNAP_NAME", Path::new("veronica")),
        ("SNAP", snap.as_path()),
    ];

    let setup = home.vr_with(
        &[
            "--json",
            "extensions",
            "setup",
            "--profile",
            "simple",
            "--dry-run",
        ],
        &env,
    );
    assert_eq!(code(&setup), 0, "{}", stderr(&setup));
    assert_eq!(document(&setup)["install"][0]["id"], "probe");

    let preview = home.vr_with(&["--json", "extensions", "install", "probe"], &env);
    assert_eq!(code(&preview), 5, "{}", stderr(&preview));
    assert_eq!(document(&preview)["preview"]["origin"], "bundled");
    let installed = home.vr_with(&["--json", "--yes", "extensions", "install", "probe"], &env);
    assert_eq!(code(&installed), 0, "{}", stderr(&installed));
    assert_eq!(document(&installed)["origin"], "bundled");
    assert_eq!(document(&installed)["version"], "1.0.2");

    let listed = document(&home.vr(&["--json", "extensions", "ls"]));
    let row = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "probe")
        .unwrap();
    assert_eq!(row["origin"], "bundled");
    let table = stdout(&home.vr(&["extensions", "ls"]));
    assert!(
        table
            .lines()
            .any(|line| line.starts_with("probe") && line.contains("bundled")),
        "{table}"
    );
    let status = document(&home.vr(&["--json", "extensions", "status"]));
    let probe = status["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "probe")
        .unwrap();
    assert_eq!(probe["origin"], "bundled");
}
