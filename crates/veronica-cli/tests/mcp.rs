mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use serde_json::{json, Value};
use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

const PATIENCE: Duration = Duration::from_mins(1);
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
    fn with_fixture() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        Installer::new(home.layout(), HostIdentity::current())
            .install_sideload(&common::fixture_source(home.root.path()))
            .unwrap();
        home
    }

    fn layout(&self) -> Layout {
        Layout::new(&AppDirectories::with_env(
            self.root.path(),
            EnvOverrides::default(),
        ))
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_vr"));
        for key in XDG {
            command.env_remove(key);
        }
        command
            .env("HOME", self.root.path())
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent/session")
            .args(args);
        command
    }

    fn vr(&self, args: &[&str]) -> i32 {
        self.command(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .code()
            .unwrap()
    }

    fn serve(&self, args: &[&str]) -> Session {
        Session::start(self.command(args))
    }
}

struct Session {
    child: Child,
    stdin: ChildStdin,
    incoming: Receiver<Value>,
    notifications: Vec<Value>,
    next: u64,
}

impl Session {
    fn start(mut command: Command) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, incoming) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let message: Value = serde_json::from_str(&line)
                    .unwrap_or_else(|error| panic!("{error}: stdout carried {line:?}"));
                if sender.send(message).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin,
            incoming,
            notifications: Vec::new(),
            next: 100,
        }
    }

    fn send_line(&mut self, line: &str) {
        writeln!(self.stdin, "{line}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn receive(&mut self) -> Value {
        self.incoming
            .recv_timeout(PATIENCE)
            .expect("the server answers in time")
    }

    fn reply_to(&mut self, id: &Value) -> Value {
        loop {
            let message = self.receive();
            if message.get("id") == Some(id) {
                return message;
            }
            assert!(message.get("id").is_none(), "unexpected reply {message}");
            self.notifications.push(message);
        }
    }

    fn request(&mut self, method: &str, params: &Value) -> Value {
        self.next += 1;
        let id = json!(self.next);
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.send_line(&message.to_string());
        self.reply_to(&id)
    }

    fn initialize(&mut self, version: &str) -> Value {
        let reply = self.request("initialize", &json!({ "protocolVersion": version, "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } }),
        );
        self.send_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        reply
    }

    fn tools(&mut self) -> Vec<Value> {
        self.request("tools/list", &json!({}))["result"]["tools"]
            .as_array()
            .unwrap()
            .clone()
    }

    fn call(&mut self, name: &str, arguments: &Value) -> Value {
        self.request(
            "tools/call",
            &json!({ "name": name, "arguments": arguments }),
        )
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn names(tools: &[Value]) -> Vec<String> {
    tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect()
}

fn find<'a>(tools: &'a [Value], name: &str) -> &'a Value {
    tools
        .iter()
        .find(|tool| tool["name"] == name)
        .unwrap_or_else(|| panic!("{name} is not listed"))
}

#[test]
fn every_supported_version_is_echoed_and_others_get_the_latest() {
    let home = Home::with_fixture();
    for version in ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"] {
        let mut session = home.serve(&["mcp"]);
        let reply = session.initialize(version);
        assert_eq!(reply["result"]["protocolVersion"], version);
        assert_eq!(
            reply["result"]["capabilities"]["tools"]["listChanged"],
            true
        );
        assert!(reply["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("veronica_catalog"));
    }
    let mut session = home.serve(&["mcp", "serve"]);
    assert_eq!(
        session.initialize("2023-01-01")["result"]["protocolVersion"],
        "2025-11-25"
    );
}

#[test]
fn protocol_errors_carry_their_codes() {
    let home = Home::with_fixture();
    let mut session = home.serve(&["mcp"]);
    assert_eq!(
        session.request("tools/list", &json!({}))["error"]["code"],
        -32600
    );
    session.send_line("{not json");
    assert_eq!(session.reply_to(&Value::Null)["error"]["code"], -32700);
    session.send_line(r#"{"jsonrpc":"1.0","id":7,"method":"ping"}"#);
    assert_eq!(session.reply_to(&json!(7))["error"]["code"], -32600);
    session.initialize("2025-11-25");
    assert_eq!(
        session.request("nothing/here", &json!({}))["error"]["code"],
        -32601
    );
    assert_eq!(
        session.call("veronica_no_such_tool", &json!({}))["error"]["code"],
        -32602
    );
    assert_eq!(session.request("ping", &json!({}))["result"], json!({}));
}

#[test]
fn tools_follow_the_command_tree() {
    let home = Home::with_fixture();
    let mut session = home.serve(&["mcp"]);
    session.initialize("2025-11-25");
    let tools = session.tools();
    let listed = names(&tools);
    for name in [
        "veronica_catalog",
        "veronica_fixture_echo",
        "veronica_fixture_wipe",
        "veronica_config_list",
        "veronica_extensions_info",
    ] {
        assert!(listed.contains(&name.to_string()), "{name} missing");
    }
    assert!(listed.iter().all(|name| name.len() <= 64));
    assert!(!listed.iter().any(|name| name.starts_with("veronica_mcp")));
    let echo = find(&tools, "veronica_fixture_echo");
    assert_eq!(echo["annotations"]["readOnlyHint"], true);
    assert!(echo["inputSchema"]["properties"]["confirm"].is_null());
    assert_eq!(
        echo["inputSchema"]["properties"]["arguments"]["type"],
        "array"
    );
    let wipe = find(&tools, "veronica_fixture_wipe");
    assert_eq!(wipe["annotations"]["destructiveHint"], true);
    assert_eq!(
        wipe["inputSchema"]["properties"]["confirm"]["type"],
        "boolean"
    );
    assert!(wipe["description"]
        .as_str()
        .unwrap()
        .contains("Usage: vr fixture wipe <item>"));
}

#[test]
fn calls_run_the_command_and_return_its_document() {
    let home = Home::with_fixture();
    let mut session = home.serve(&["mcp"]);
    session.initialize("2025-11-25");

    let echoed = session.call("veronica_fixture_echo", &json!({ "arguments": ["hello"] }));
    let result = &echoed["result"];
    assert_eq!(result["isError"], false, "{echoed}");
    assert_eq!(result["structuredContent"]["text"], "hello");
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text["text"], "hello");

    let preview = session.call("veronica_fixture_wipe", &json!({ "arguments": ["cache"] }));
    assert_eq!(preview["result"]["isError"], false, "{preview}");
    assert_eq!(
        preview["result"]["structuredContent"]["confirmationRequired"],
        true
    );
    assert_eq!(
        preview["result"]["structuredContent"]["preview"]["wouldRemove"],
        "cache"
    );

    let confirmed = session.call(
        "veronica_fixture_wipe",
        &json!({ "arguments": ["cache"], "confirm": true }),
    );
    assert_eq!(confirmed["result"]["isError"], false, "{confirmed}");
    assert_eq!(
        confirmed["result"]["structuredContent"],
        json!({ "removed": "cache" })
    );

    let failed = session.call(
        "veronica_fixture_fail",
        &json!({ "arguments": ["--code", "6"] }),
    );
    assert_eq!(failed["result"]["isError"], true, "{failed}");
    assert!(!failed["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .is_empty());
}

#[test]
fn host_flags_in_arguments_are_refused() {
    let home = Home::with_fixture();
    let mut session = home.serve(&["mcp"]);
    session.initialize("2025-11-25");
    for word in ["--yes", "-y", "--json"] {
        let reply = session.call(
            "veronica_fixture_wipe",
            &json!({ "arguments": ["cache", word] }),
        );
        assert_eq!(reply["error"]["code"], -32602, "{word}: {reply}");
    }
    let reply = session.call(
        "veronica_fixture_echo",
        &json!({ "arguments": ["x"], "confirm": true }),
    );
    assert_eq!(reply["error"]["code"], -32602);
}

#[test]
fn structured_content_starts_with_2025_06_18() {
    let home = Home::with_fixture();
    let mut session = home.serve(&["mcp"]);
    session.initialize("2025-03-26");
    let echoed = session.call("veronica_fixture_echo", &json!({ "arguments": ["old"] }));
    assert!(echoed["result"].get("structuredContent").is_none());
    assert!(echoed["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("old"));
}

#[test]
fn the_catalog_tool_previews_changes_until_confirmed() {
    let home = Home::with_fixture();
    let mut session = home.serve(&["mcp"]);
    session.initialize("2025-11-25");
    let listed = session.call(
        "veronica_catalog",
        &json!({ "action": "list", "status": "enabled" }),
    );
    let rows = listed["result"]["content"][0]["text"].as_str().unwrap();
    let rows: Value = serde_json::from_str(rows).unwrap();
    assert!(rows
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["id"] == "fixture"));
    let preview = session.call(
        "veronica_catalog",
        &json!({ "action": "disable", "id": "fixture" }),
    );
    assert_eq!(
        preview["result"]["structuredContent"]["confirmationRequired"],
        true
    );
    assert!(names(&session.tools()).contains(&"veronica_fixture_echo".to_string()));
    let disabled = session.call(
        "veronica_catalog",
        &json!({ "action": "disable", "id": "fixture", "confirm": true }),
    );
    assert_eq!(disabled["result"]["isError"], false, "{disabled}");
    assert!(!names(&session.tools()).contains(&"veronica_fixture_echo".to_string()));
    assert_eq!(
        session.call("veronica_catalog", &json!({ "action": "info" }))["error"]["code"],
        -32602
    );
}

#[test]
fn disabled_extensions_have_no_tools() {
    let home = Home::with_fixture();
    assert_eq!(home.vr(&["extensions", "disable", "fixture"]), 0);
    let mut session = home.serve(&["mcp"]);
    session.initialize("2025-11-25");
    let listed = names(&session.tools());
    assert!(!listed
        .iter()
        .any(|name| name.starts_with("veronica_fixture")));
}
