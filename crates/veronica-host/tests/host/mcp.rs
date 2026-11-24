use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::PathBuf;

use serde_json::{json, Value};
use tempfile::TempDir;
use veronica_host::mcp::clients::{Client, Homes, Selection};
use veronica_host::mcp::{install, status, uninstall, Action, Report, ServerEntry};

struct Home {
    root: TempDir,
}

impl Home {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
        }
    }

    fn homes(&self) -> Homes {
        Homes {
            home: self.root.path().to_path_buf(),
            codex_home: None,
            config_home: None,
        }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }

    fn mkdir(&self, relative: &str) {
        fs::create_dir_all(self.path(relative)).unwrap();
    }

    fn write(&self, relative: &str, body: &str) -> PathBuf {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, body).unwrap();
        path
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path(relative)).unwrap()
    }

    fn backups(&self, directory: &str) -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = fs::read_dir(self.path(directory))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.to_string_lossy().contains(".veronica-backup-"))
            .collect();
        found.sort();
        found
    }
}

fn server(command: &str) -> ServerEntry {
    ServerEntry {
        command: command.to_string(),
        args: vec!["mcp".to_string()],
    }
}

fn only(report: &Report) -> Action {
    assert_eq!(report.clients.len(), 1);
    report.clients[0].action
}

struct Layout {
    client: Client,
    marker: &'static str,
    file: &'static str,
    directory: &'static str,
}

const LAYOUTS: [Layout; 4] = [
    Layout {
        client: Client::Claude,
        marker: ".claude",
        file: ".claude.json",
        directory: "",
    },
    Layout {
        client: Client::Codex,
        marker: ".codex",
        file: ".codex/config.toml",
        directory: ".codex",
    },
    Layout {
        client: Client::Cursor,
        marker: ".cursor",
        file: ".cursor/mcp.json",
        directory: ".cursor",
    },
    Layout {
        client: Client::Vscode,
        marker: ".config/Code/User",
        file: ".config/Code/User/mcp.json",
        directory: ".config/Code/User",
    },
];

fn registered(home: &Home, layout: &Layout) -> Option<(String, Vec<String>)> {
    let text = fs::read_to_string(home.path(layout.file)).ok()?;
    if layout.client == Client::Codex {
        let document: toml_edit::DocumentMut = text.parse().unwrap();
        let entry = document.get("mcp_servers")?.get("veronica")?;
        let args = entry["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|arg| arg.as_str().unwrap().to_string())
            .collect();
        return Some((entry["command"].as_str().unwrap().to_string(), args));
    }
    let key = if layout.client == Client::Vscode {
        "servers"
    } else {
        "mcpServers"
    };
    let document: Value = serde_json::from_str(&text).unwrap();
    let entry = document.get(key)?.get("veronica")?;
    assert_eq!(entry["type"], "stdio");
    let args = serde_json::from_value(entry["args"].clone()).unwrap();
    Some((entry["command"].as_str().unwrap().to_string(), args))
}

#[test]
fn every_client_goes_through_create_update_unchanged_and_remove() {
    for layout in &LAYOUTS {
        let home = Home::new();
        home.mkdir(layout.marker);
        let homes = home.homes();
        let selection = Selection::One(layout.client);
        let name = layout.client.name();

        let created = install(&homes, selection, &server("/usr/bin/vr"), false);
        assert_eq!(only(&created), Action::Created, "{name}");
        assert_eq!(created.clients[0].path, home.path(layout.file));
        assert_eq!(
            registered(&home, layout),
            Some(("/usr/bin/vr".to_string(), vec!["mcp".to_string()])),
            "{name}"
        );
        assert!(created.succeeded());

        let again = install(&homes, selection, &server("/usr/bin/vr"), false);
        assert_eq!(only(&again), Action::Unchanged, "{name}");
        assert!(home.backups(layout.directory).is_empty(), "{name}");

        let before = home.read(layout.file);
        let updated = install(&homes, selection, &server("/opt/vr"), false);
        assert_eq!(only(&updated), Action::Updated, "{name}");
        assert_eq!(registered(&home, layout).unwrap().0, "/opt/vr", "{name}");
        let backups = home.backups(layout.directory);
        assert_eq!(backups.len(), 1, "{name}");
        assert_eq!(updated.clients[0].backup.as_ref(), Some(&backups[0]));
        assert_eq!(fs::read_to_string(&backups[0]).unwrap(), before);

        let removed = uninstall(&homes, selection, false);
        assert_eq!(only(&removed), Action::Removed, "{name}");
        assert_eq!(registered(&home, layout), None, "{name}");
        let absent = uninstall(&homes, selection, false);
        assert_eq!(only(&absent), Action::Unchanged, "{name}");
    }
}

#[test]
fn json_edits_keep_other_servers_settings_order_and_formatting() {
    let home = Home::new();
    let original = "{\n    \"numStartups\": 7,\n    \"mcpServers\": {\n        \"other\": {\n            \"command\": \"other-server\"\n        }\n    },\n    \"projects\": {\n        \"/work\": {\n            \"mcpServers\": {}\n        }\n    }\n}\n";
    let path = home.write(".claude.json", original);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let report = install(
        &home.homes(),
        Selection::One(Client::Claude),
        &server("/usr/bin/vr"),
        false,
    );
    assert_eq!(only(&report), Action::Created);
    let text = home.read(".claude.json");
    let positions: Vec<usize> = [
        "\"numStartups\"",
        "\"mcpServers\"",
        "\"other\"",
        "\"veronica\"",
        "\"projects\"",
    ]
    .iter()
    .map(|key| text.find(key).unwrap())
    .collect();
    assert!(positions.is_sorted(), "{text}");
    assert!(text.contains("\n    \"numStartups\": 7,\n"));
    assert!(text.contains("\n        \"veronica\": {"), "{text}");
    let document: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        document["mcpServers"]["other"],
        json!({"command": "other-server"})
    );
    assert_eq!(document["projects"]["/work"]["mcpServers"], json!({}));
    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode, 0o600);

    uninstall(&home.homes(), Selection::One(Client::Claude), false);
    let document: Value = serde_json::from_str(&home.read(".claude.json")).unwrap();
    assert_eq!(
        document["mcpServers"],
        json!({"other": {"command": "other-server"}})
    );
    assert_eq!(document["numStartups"], 7);
}

#[test]
fn updating_keeps_extra_fields_on_the_veronica_entry() {
    let home = Home::new();
    home.write(
        ".cursor/mcp.json",
        r#"{"mcpServers":{"veronica":{"command":"/old/vr","args":["mcp"],"env":{"LOG":"1"}}}}"#,
    );
    let report = install(
        &home.homes(),
        Selection::One(Client::Cursor),
        &server("/usr/bin/vr"),
        false,
    );
    assert_eq!(only(&report), Action::Updated);
    let document: Value = serde_json::from_str(&home.read(".cursor/mcp.json")).unwrap();
    assert_eq!(
        document["mcpServers"]["veronica"],
        json!({"command": "/usr/bin/vr", "args": ["mcp"], "env": {"LOG": "1"}, "type": "stdio"})
    );
}

#[test]
fn toml_edits_keep_comments_settings_and_other_servers() {
    let home = Home::new();
    let original = "# personal settings\nmodel = \"fast\"\n\n[mcp_servers.other]\ncommand = \"other-server\" # keep me\nargs = []\n\n[profiles.work]\nmodel = \"slow\"\n";
    home.write(".codex/config.toml", original);
    let report = install(
        &home.homes(),
        Selection::One(Client::Codex),
        &server("/usr/bin/vr"),
        false,
    );
    assert_eq!(only(&report), Action::Created);
    let text = home.read(".codex/config.toml");
    for kept in [
        "# personal settings\nmodel = \"fast\"\n",
        "[mcp_servers.other]\ncommand = \"other-server\" # keep me\nargs = []\n",
        "[profiles.work]\nmodel = \"slow\"\n",
    ] {
        assert!(text.contains(kept), "{text}");
    }
    assert!(
        text.contains("[mcp_servers.veronica]\ncommand = \"/usr/bin/vr\"\nargs = [\"mcp\"]\n"),
        "{text}"
    );

    uninstall(&home.homes(), Selection::One(Client::Codex), false);
    assert_eq!(home.read(".codex/config.toml"), original);
}

#[test]
fn codex_home_overrides_the_default_location() {
    let home = Home::new();
    home.mkdir("elsewhere");
    let homes = Homes {
        codex_home: Some(home.path("elsewhere")),
        ..home.homes()
    };
    let report = install(&homes, Selection::One(Client::Codex), &server("vr"), false);
    assert_eq!(only(&report), Action::Created);
    assert!(home.path("elsewhere/config.toml").is_file());
    assert!(!home.path(".codex").exists());
}

#[test]
fn symlinked_configs_are_updated_through_the_link() {
    let home = Home::new();
    let target = home.write("dotfiles/cursor.json", "{\"mcpServers\": {}}\n");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
    home.mkdir(".cursor");
    symlink("../dotfiles/cursor.json", home.path(".cursor/mcp.json")).unwrap();
    let report = install(
        &home.homes(),
        Selection::One(Client::Cursor),
        &server("/usr/bin/vr"),
        false,
    );
    assert_eq!(only(&report), Action::Created);
    assert!(home.path(".cursor/mcp.json").is_symlink());
    let document: Value = serde_json::from_str(&home.read("dotfiles/cursor.json")).unwrap();
    assert_eq!(document["mcpServers"]["veronica"]["command"], "/usr/bin/vr");
    assert_eq!(home.backups("dotfiles").len(), 1);
    assert!(home.backups(".cursor").is_empty());
    let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode, 0o640);
}

#[test]
fn json_with_comments_or_trailing_commas_is_refused_with_a_snippet() {
    let home = Home::new();
    let commented = "{\n  // mine\n  \"servers\": {}\n}\n";
    home.write(".config/Code/User/mcp.json", commented);
    let trailing = "{\"mcpServers\": {\"a\": {\"command\": \"a\"},},}";
    home.write(".cursor/mcp.json", trailing);
    let report = install(&home.homes(), Selection::All, &server("/usr/bin/vr"), false);
    assert!(!report.succeeded());
    for client in &report.clients {
        match client.client {
            Client::Vscode | Client::Cursor => {
                assert_eq!(client.action, Action::Refused);
                assert!(client.reason.as_ref().unwrap().contains("comments"));
            }
            Client::Claude | Client::Codex => assert_eq!(client.action, Action::Skipped),
        }
    }
    let vscode = report
        .clients
        .iter()
        .find(|c| c.client == Client::Vscode)
        .unwrap();
    let snippet: Value = serde_json::from_str(vscode.snippet.as_ref().unwrap()).unwrap();
    assert_eq!(
        snippet,
        json!({"servers": {"veronica": {"type": "stdio", "command": "/usr/bin/vr", "args": ["mcp"]}}})
    );
    assert_eq!(home.read(".config/Code/User/mcp.json"), commented);
    assert_eq!(home.read(".cursor/mcp.json"), trailing);
    assert!(home.backups(".cursor").is_empty());
}

#[test]
fn broken_or_mistyped_toml_is_refused() {
    for original in [
        "model = \n",
        "[mcp_servers]\nveronica = \"vr\"\n",
        "mcp_servers = 3\n",
    ] {
        let home = Home::new();
        home.write(".codex/config.toml", original);
        let report = install(
            &home.homes(),
            Selection::One(Client::Codex),
            &server("/usr/bin/vr"),
            false,
        );
        assert_eq!(only(&report), Action::Refused, "{original}");
        let snippet = report.clients[0].snippet.as_ref().unwrap();
        assert!(snippet.contains("[mcp_servers.veronica]"), "{snippet}");
        assert_eq!(home.read(".codex/config.toml"), original);
        assert_eq!(
            only(&uninstall(
                &home.homes(),
                Selection::One(Client::Codex),
                false
            )),
            Action::Refused
        );
    }
}

#[test]
fn dry_run_writes_nothing_and_shows_the_snippet() {
    let home = Home::new();
    home.mkdir(".claude");
    let existing = home.write(".cursor/mcp.json", "{\"mcpServers\": {}}");
    let report = install(&home.homes(), Selection::All, &server("/usr/bin/vr"), true);
    assert!(report.dry_run);
    let actions: Vec<(Client, Action)> = report
        .clients
        .iter()
        .map(|c| (c.client, c.action))
        .collect();
    assert_eq!(
        actions,
        [
            (Client::Claude, Action::Created),
            (Client::Codex, Action::Skipped),
            (Client::Cursor, Action::Created),
            (Client::Vscode, Action::Skipped),
        ]
    );
    assert!(report.clients[0]
        .snippet
        .as_ref()
        .unwrap()
        .contains("mcpServers"));
    assert!(!home.path(".claude.json").exists());
    assert_eq!(
        fs::read_to_string(&existing).unwrap(),
        "{\"mcpServers\": {}}"
    );
    assert!(home.backups(".cursor").is_empty());

    install(&home.homes(), Selection::All, &server("/usr/bin/vr"), false);
    let before = home.read(".cursor/mcp.json");
    let removal = uninstall(&home.homes(), Selection::All, true);
    assert_eq!(removal.clients[2].action, Action::Removed);
    assert_eq!(home.read(".cursor/mcp.json"), before);
}

#[test]
fn all_skips_missing_clients_and_a_named_missing_client_is_not_found() {
    let home = Home::new();
    home.mkdir(".codex");
    let report = install(&home.homes(), Selection::All, &server("/usr/bin/vr"), false);
    let actions: Vec<Action> = report.clients.iter().map(|c| c.action).collect();
    assert_eq!(
        actions,
        [
            Action::Skipped,
            Action::Created,
            Action::Skipped,
            Action::Skipped
        ]
    );
    assert!(report.succeeded());

    let missing = install(
        &home.homes(),
        Selection::One(Client::Cursor),
        &server("/usr/bin/vr"),
        false,
    );
    assert_eq!(only(&missing), Action::NotFound);
    assert!(!missing.succeeded());
    assert!(!home.path(".cursor").exists());
}

#[test]
fn status_reports_presence_and_drift() {
    let home = Home::new();
    home.mkdir(".claude");
    home.write(
        ".cursor/mcp.json",
        r#"{"mcpServers":{"veronica":{"type":"stdio","command":"/old/vr","args":["mcp"]}}}"#,
    );
    home.write(".codex/config.toml", "model = \n");
    install(
        &home.homes(),
        Selection::One(Client::Claude),
        &server("/usr/bin/vr"),
        false,
    );
    let report = status(&home.homes(), Selection::All, &server("/usr/bin/vr"));
    let summary: Vec<(Client, bool, bool, bool, bool)> = report
        .clients
        .iter()
        .map(|c| (c.client, c.found, c.present, c.matches, c.reason.is_some()))
        .collect();
    assert_eq!(
        summary,
        [
            (Client::Claude, true, true, true, false),
            (Client::Codex, true, false, false, true),
            (Client::Cursor, true, true, false, false),
            (Client::Vscode, false, false, false, false),
        ]
    );
}

#[test]
fn selections_parse_from_client_names() {
    assert_eq!("all".parse::<Selection>().unwrap(), Selection::All);
    assert_eq!(
        "vscode".parse::<Selection>().unwrap(),
        Selection::One(Client::Vscode)
    );
    assert!("emacs".parse::<Selection>().is_err());
}

#[test]
fn reports_serialize_with_camel_case_actions() {
    let home = Home::new();
    let report = install(
        &home.homes(),
        Selection::One(Client::Cursor),
        &server("/usr/bin/vr"),
        true,
    );
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(value["dryRun"], true);
    assert_eq!(value["clients"][0]["action"], "notFound");
    assert_eq!(value["clients"][0]["client"], "cursor");
    assert_eq!(value["server"]["args"], json!(["mcp"]));
}
