use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use veronica_protocol::rpc::{error_code, MAX_MESSAGE_BYTES, PROTOCOL_VERSION};

const DEADLINE: Duration = Duration::from_secs(20);

fn echo_backend() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let output = std::process::Command::new(cargo)
            .args([
                "build",
                "--quiet",
                "--package",
                "veronica-sdk",
                "--example",
                "echo",
                "--message-format=json",
            ])
            .stderr(Stdio::inherit())
            .output()
            .expect("cargo runs");
        assert!(output.status.success(), "building the echo example failed");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|message| message["target"]["name"] == "echo")
            .find_map(|message| message["executable"].as_str().map(PathBuf::from))
            .expect("cargo reports the echo executable")
    })
}

struct Backend {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Lines<BufReader<ChildStdout>>,
}

impl Backend {
    fn spawn() -> Self {
        let mut child = Command::new(echo_backend())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("the echo backend starts");
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap()).lines();
        Self {
            child,
            stdin,
            stdout,
        }
    }

    async fn ready() -> Self {
        let mut backend = Self::spawn();
        let result = backend.request(0, "initialize", initialize_params()).await;
        assert_eq!(result["result"]["protocol"], json!(PROTOCOL_VERSION));
        backend
            .send(&json!({ "jsonrpc": "2.0", "method": "initialized" }))
            .await;
        backend
    }

    async fn send_raw(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        stdin.write_all(bytes).await.unwrap();
        stdin.flush().await.unwrap();
    }

    async fn send(&mut self, message: &Value) {
        let mut line = serde_json::to_vec(message).unwrap();
        line.push(b'\n');
        self.send_raw(&line).await;
    }

    async fn recv(&mut self) -> Value {
        let line = tokio::time::timeout(DEADLINE, self.stdout.next_line())
            .await
            .expect("the backend answers in time")
            .unwrap()
            .expect("the backend is still writing");
        serde_json::from_str(&line).unwrap()
    }

    async fn request(&mut self, id: i64, method: &str, params: Value) -> Value {
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await;
        let response = self.recv().await;
        assert_eq!(response["id"], json!(id), "unexpected message {response}");
        response
    }

    async fn ping(&mut self, id: i64) {
        let response = self.request(id, "health.ping", Value::Null).await;
        assert_eq!(
            response,
            json!({ "jsonrpc": "2.0", "id": id, "result": null })
        );
    }

    async fn exit_status(&mut self) -> ExitStatus {
        tokio::time::timeout(DEADLINE, self.child.wait())
            .await
            .expect("the backend exits in time")
            .unwrap()
    }
}

fn echo_install() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("echo-install");
        std::fs::create_dir_all(&root).unwrap();
        let manifest = json!({
            "schema": 1,
            "id": "echo",
            "version": "1.0",
            "title": "Echo",
            "summary": "Says things back.",
            "category": "labs",
            "engines": { "host": ">=0.1.0", "hostApi": 1 },
            "targets": ["linux-x86_64"],
            "backend": { "entry": "bin/backend" },
            "commands": [{
                "name": "echo",
                "subcommands": [
                    { "name": "say", "effect": "read", "json": true },
                    { "name": "fail", "effect": "read", "json": true,
                      "args": [{ "name": "code", "kind": "option", "long": "code", "type": "integer" }] },
                    { "name": "erase", "effect": "destructive", "json": true }
                ]
            }]
        });
        std::fs::write(root.join("manifest.json"), manifest.to_string()).unwrap();
        root
    })
}

fn initialize_params() -> Value {
    json!({
        "protocol": PROTOCOL_VERSION,
        "host": { "version": "0.1.26", "apiVersion": 1 },
        "extension": { "id": "echo", "version": "1.0.0" },
        "mode": "service",
        "dirs": {
            "install": echo_install(),
            "data": "/ext/data",
            "cache": "/ext/cache",
            "state": "/ext/state",
            "runtime": "/ext/runtime"
        },
        "granted": {},
        "session": { "kind": "wayland", "packaging": "deb" },
        "locale": "en"
    })
}

fn error_code_of(response: &Value) -> i64 {
    response["error"]["code"]
        .as_i64()
        .unwrap_or_else(|| panic!("expected an error, got {response}"))
}

fn cli(argv: &[&str], json: bool) -> Value {
    json!({ "argv": argv, "json": json, "confirm": false, "cwd": "/", "tty": false })
}

fn confirmed(argv: &[&str], json: bool) -> Value {
    json!({ "argv": argv, "json": json, "confirm": true, "cwd": "/", "tty": false })
}

#[tokio::test]
async fn the_handshake_comes_before_everything_else() {
    let mut backend = Backend::spawn();
    let early = backend.request(1, "health.ping", Value::Null).await;
    assert_eq!(error_code_of(&early), error_code::INVALID_REQUEST);

    let initialized = backend.request(2, "initialize", initialize_params()).await;
    assert_eq!(
        initialized["result"],
        json!({
            "protocol": PROTOCOL_VERSION,
            "methods": ["echo", "ask-host", "dirs", "sleep"],
            "topics": []
        })
    );
    backend
        .send(&json!({ "jsonrpc": "2.0", "method": "initialized" }))
        .await;

    let again = backend.request(3, "initialize", initialize_params()).await;
    assert_eq!(error_code_of(&again), error_code::INVALID_REQUEST);
    backend.ping(4).await;

    let dirs = backend
        .request(
            5,
            "rpc.call",
            json!({ "method": "dirs", "caller": { "kind": "host" } }),
        )
        .await;
    assert_eq!(
        dirs["result"],
        json!({ "data": "/ext/data", "mode": "service" })
    );
}

#[tokio::test]
async fn a_protocol_mismatch_is_refused_and_the_backend_exits() {
    let mut backend = Backend::spawn();
    let mut params = initialize_params();
    params["protocol"] = json!(PROTOCOL_VERSION + 1);
    let response = backend.request(1, "initialize", params).await;
    assert_eq!(error_code_of(&response), error_code::VERSION_MISMATCH);
    assert_eq!(backend.exit_status().await.code(), Some(9));
}

#[tokio::test]
async fn cli_run_captures_documents_text_and_exit_codes() {
    let mut backend = Backend::ready().await;

    let json_run = backend
        .request(1, "cli.run", cli(&["echo", "say", "hi", "--upper"], true))
        .await;
    assert_eq!(
        json_run["result"],
        json!({ "exitCode": 0, "document": { "text": "HI" } })
    );

    let text_run = backend
        .request(2, "cli.run", cli(&["echo", "say", "hi"], false))
        .await;
    assert_eq!(json!({ "exitCode": 0, "text": "hi\n" }), text_run["result"]);

    let failed = backend
        .request(3, "cli.run", cli(&["echo", "fail", "--code", "3"], true))
        .await;
    assert_eq!(
        failed["result"],
        json!({ "exitCode": 3, "document": { "error": { "code": 3, "message": "failed with 3" } } })
    );

    let usage = backend
        .request(4, "cli.run", cli(&["echo", "say"], false))
        .await;
    assert_eq!(usage["result"]["exitCode"], json!(2));
    assert!(usage["result"]["text"].as_str().unwrap().contains("<TEXT>"));
}

#[tokio::test]
async fn destructive_leaves_run_only_when_confirmed() {
    let mut backend = Backend::ready().await;
    let refused = backend
        .request(1, "cli.run", cli(&["echo", "erase", "notes"], true))
        .await;
    assert_eq!(refused["result"]["exitCode"], json!(5));
    assert_eq!(
        refused["result"]["document"]["confirmationRequired"],
        json!(true)
    );

    let text = backend
        .request(2, "cli.run", cli(&["echo", "erase", "notes"], false))
        .await;
    assert_eq!(text["result"]["exitCode"], json!(5));
    assert!(text["result"]["text"]
        .as_str()
        .unwrap()
        .contains("confirmation"));

    let erased = backend
        .request(3, "cli.run", confirmed(&["echo", "erase", "notes"], true))
        .await;
    assert_eq!(
        erased["result"],
        json!({ "exitCode": 0, "document": { "erased": "notes" } })
    );

    let read = backend
        .request(4, "cli.run", cli(&["echo", "--json", "say", "erase"], true))
        .await;
    assert_eq!(read["result"]["document"], json!({ "text": "erase" }));
}

#[tokio::test]
async fn rpc_call_reaches_the_extension() {
    let mut backend = Backend::ready().await;
    let echoed = backend
        .request(
            1,
            "rpc.call",
            json!({ "method": "echo", "params": { "a": [1, 2] }, "caller": { "kind": "ui" } }),
        )
        .await;
    assert_eq!(echoed["result"], json!({ "a": [1, 2] }));

    let unknown = backend
        .request(
            2,
            "rpc.call",
            json!({ "method": "nope", "caller": { "kind": "cli" } }),
        )
        .await;
    assert_eq!(error_code_of(&unknown), error_code::METHOD_NOT_FOUND);

    let unhandled = backend
        .request(3, "shortcut.fired", json!({ "id": "toggle" }))
        .await;
    assert_eq!(error_code_of(&unhandled), error_code::METHOD_NOT_FOUND);

    let bad_params = backend.request(4, "cli.run", json!({ "argv": 5 })).await;
    assert_eq!(error_code_of(&bad_params), error_code::INVALID_PARAMS);
}

#[tokio::test]
async fn the_host_client_correlates_requests_with_host_responses() {
    let mut backend = Backend::ready().await;
    backend
        .send(&json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "rpc.call",
            "params": { "method": "ask-host", "params": { "key": "theme" }, "caller": { "kind": "ui" } }
        }))
        .await;

    let outbound = backend.recv().await;
    assert_eq!(outbound["method"], json!("host.settings.get"));
    assert_eq!(outbound["params"], json!({ "key": "theme" }));
    let host_id = outbound["id"].clone();
    assert!(host_id.is_number());
    backend
        .send(&json!({ "jsonrpc": "2.0", "id": host_id, "result": { "value": "dark" } }))
        .await;

    let event = backend.recv().await;
    assert_eq!(
        event,
        json!({
            "jsonrpc": "2.0",
            "method": "event.emit",
            "params": { "topic": "echo.asked", "payload": { "key": "theme" } }
        })
    );
    let answer = backend.recv().await;
    assert_eq!(answer["id"], json!(7));
    assert_eq!(answer["result"], json!({ "value": "dark" }));
}

#[tokio::test]
async fn a_host_error_reaches_the_caller() {
    let mut backend = Backend::ready().await;
    backend
        .send(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "rpc.call",
            "params": { "method": "ask-host", "params": { "key": "secret" }, "caller": { "kind": "ui" } }
        }))
        .await;
    let outbound = backend.recv().await;
    backend
        .send(&json!({
            "jsonrpc": "2.0",
            "id": outbound["id"],
            "error": { "code": error_code::PERMISSION_DENIED, "message": "not granted" }
        }))
        .await;
    let answer = backend.recv().await;
    assert_eq!(answer["id"], json!(1));
    assert_eq!(error_code_of(&answer), error_code::PERMISSION_DENIED);
}

#[tokio::test]
async fn cancel_stops_a_running_request() {
    let mut backend = Backend::ready().await;
    backend
        .send(&json!({
            "jsonrpc": "2.0",
            "id": "slow",
            "method": "rpc.call",
            "params": { "method": "sleep", "caller": { "kind": "host" } }
        }))
        .await;
    backend.ping(1).await;
    backend
        .send(&json!({ "jsonrpc": "2.0", "method": "$/cancel", "params": { "id": "slow" } }))
        .await;
    let cancelled = backend.recv().await;
    assert_eq!(cancelled["id"], json!("slow"));
    assert_eq!(error_code_of(&cancelled), error_code::REQUEST_CANCELLED);

    backend
        .send(&json!({ "jsonrpc": "2.0", "method": "$/cancel", "params": { "id": "slow" } }))
        .await;
    backend.ping(2).await;
}

#[tokio::test]
async fn malformed_lines_get_errors_and_the_loop_continues() {
    let mut backend = Backend::ready().await;
    backend.send_raw(b"this is not json\n").await;
    let parse = backend.recv().await;
    assert_eq!(parse["id"], Value::Null);
    assert_eq!(error_code_of(&parse), error_code::PARSE_ERROR);

    backend.send(&json!({ "jsonrpc": "2.0" })).await;
    assert_eq!(
        error_code_of(&backend.recv().await),
        error_code::INVALID_REQUEST
    );

    backend
        .send(&json!({ "jsonrpc": "1.0", "id": 1, "method": "health.ping" }))
        .await;
    assert_eq!(
        error_code_of(&backend.recv().await),
        error_code::INVALID_REQUEST
    );

    backend.send_raw(b"\n\n").await;
    backend.ping(2).await;
}

#[tokio::test]
async fn an_oversize_line_is_rejected_without_killing_the_backend() {
    let mut backend = Backend::ready().await;
    let mut line = vec![b'x'; MAX_MESSAGE_BYTES + 1];
    line.push(b'\n');
    backend.send_raw(&line).await;
    let rejected = backend.recv().await;
    assert_eq!(rejected["id"], Value::Null);
    assert_eq!(error_code_of(&rejected), error_code::INVALID_REQUEST);
    backend.ping(1).await;
}

#[tokio::test]
async fn closing_stdin_exits_cleanly() {
    let mut backend = Backend::ready().await;
    backend.stdin = None;
    assert!(backend.exit_status().await.success());
}

#[tokio::test]
async fn closing_stdin_before_the_handshake_exits_cleanly() {
    let mut backend = Backend::spawn();
    backend.stdin = None;
    assert!(backend.exit_status().await.success());
}

#[tokio::test]
async fn shutdown_answers_then_exits() {
    let mut backend = Backend::ready().await;
    let response = backend.request(1, "shutdown", Value::Null).await;
    assert_eq!(response["result"], Value::Null);
    assert!(backend.exit_status().await.success());
}
