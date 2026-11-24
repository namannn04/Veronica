use serde_json::{json, Value};
use veronica_protocol::manifest::{HostPermission, Permissions, ShellService};
use veronica_protocol::rpc::{
    error_code, Caller, CallerKind, Cancel, CliRun, CliRunParams, CliRunResult, ErrorObject,
    EventEmit, ExtensionDirs, ExtensionInfo, HealthPing, HostInfo, HostShellAction, Initialize,
    InitializeParams, InitializeResult, Initialized, Message, Notification, Outcome, Progress,
    ProgressParams, Request, RequestId, RequestMethod, Response, RpcCall, RunMode, SessionInfo,
    Shutdown, TopicParams, MAX_MESSAGE_BYTES,
};
use veronica_protocol::ExitCode;

fn wire(message: &Message) -> Value {
    let line = message.to_line().unwrap();
    assert!(line.ends_with('\n'));
    assert_eq!(line.matches('\n').count(), 1);
    serde_json::from_str(&line).unwrap()
}

fn initialize_params() -> InitializeParams {
    InitializeParams {
        protocol: 1,
        host: HostInfo {
            version: "0.2.0".to_string(),
            api_version: 1,
        },
        extension: ExtensionInfo {
            id: "media".to_string(),
            version: "1.0.7".to_string(),
        },
        mode: RunMode::Service,
        dirs: ExtensionDirs {
            install: "/x/packages/media/current".into(),
            data: "/x/data/media".into(),
            cache: "/c/media".into(),
            state: "/s/media".into(),
            runtime: "/r/media".into(),
        },
        granted: Permissions {
            host: vec![HostPermission::Surfaces],
            ..Permissions::default()
        },
        session: SessionInfo {
            kind: "wayland".to_string(),
            desktop: Some("GNOME".to_string()),
            packaging: "deb".to_string(),
        },
        locale: "en_US".to_string(),
    }
}

#[test]
fn initialize_request_has_the_documented_shape() {
    let request = Request::new::<Initialize>(RequestId::Number(1), &initialize_params()).unwrap();
    let value = wire(&Message::Request(request.clone()));
    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 1);
    assert_eq!(value["method"], "initialize");
    assert_eq!(
        value["params"]["host"],
        json!({ "version": "0.2.0", "apiVersion": 1 })
    );
    assert_eq!(value["params"]["mode"], "service");
    assert_eq!(value["params"]["dirs"]["data"], "/x/data/media");
    assert_eq!(value["params"]["granted"]["host"], json!(["surfaces"]));
    let parsed = Message::parse(&value.to_string()).unwrap();
    let Message::Request(parsed) = parsed else {
        panic!("expected a request");
    };
    assert_eq!(parsed, request);
    assert_eq!(parsed.params::<Initialize>().unwrap(), initialize_params());
    assert_eq!(
        parsed.params::<Shutdown>().unwrap_err().code,
        error_code::METHOD_NOT_FOUND
    );
}

#[test]
fn unit_params_are_omitted_and_accepted_when_missing() {
    let request = Request::new::<HealthPing>(RequestId::String("p".to_string()), &()).unwrap();
    let value = wire(&Message::Request(request.clone()));
    assert_eq!(
        value,
        json!({ "jsonrpc": "2.0", "id": "p", "method": "health.ping" })
    );
    request.params::<HealthPing>().unwrap();
    let initialized = Notification::new::<Initialized>(&()).unwrap();
    assert_eq!(
        wire(&Message::Notification(initialized)),
        json!({ "jsonrpc": "2.0", "method": "initialized" })
    );
}

#[test]
fn responses_carry_result_or_error() {
    let result = InitializeResult {
        protocol: 1,
        methods: vec!["library.list".to_string()],
        topics: vec!["media.nowPlaying@1".to_string()],
    };
    let ok = Response::success::<Initialize>(RequestId::Number(1), &result).unwrap();
    let value = wire(&Message::Response(ok.clone()));
    assert_eq!(value["result"]["protocol"], 1);
    assert!(value.get("error").is_none());
    let Message::Response(parsed) = Message::parse(&value.to_string()).unwrap() else {
        panic!("expected a response");
    };
    assert_eq!(parsed, ok);
    assert_eq!(parsed.result::<Initialize>().unwrap(), result);

    let failed = Response::failure(
        Some(RequestId::Number(2)),
        ErrorObject::new(error_code::PERMISSION_DENIED, "notifications not granted")
            .with_data(json!({ "permission": "notifications" })),
    );
    let value = wire(&Message::Response(failed));
    assert_eq!(
        value,
        json!({ "jsonrpc": "2.0", "id": 2, "error": {
            "code": -32001, "message": "notifications not granted",
            "data": { "permission": "notifications" } } })
    );
    let Message::Response(parsed) = Message::parse(&value.to_string()).unwrap() else {
        panic!("expected a response");
    };
    assert_eq!(
        parsed.result::<RpcCall>().unwrap_err().code,
        error_code::PERMISSION_DENIED
    );
}

#[test]
fn null_results_and_null_ids_survive() {
    let Message::Response(done) =
        Message::parse(r#"{"jsonrpc":"2.0","id":3,"result":null}"#).unwrap()
    else {
        panic!("expected a response");
    };
    assert_eq!(done.outcome, Outcome::Result(Value::Null));
    done.result::<Shutdown>().unwrap();
    let Message::Response(parse_error) =
        Message::parse(r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"bad"}}"#)
            .unwrap()
    else {
        panic!("expected a response");
    };
    assert_eq!(parse_error.id, None);
}

#[test]
fn cli_run_round_trips_with_exit_codes() {
    let params = CliRunParams {
        argv: vec![
            "library".to_string(),
            "--limit".to_string(),
            "5".to_string(),
        ],
        json: true,
        confirm: false,
        stdin: None,
        cwd: "/home/u".into(),
        tty: false,
    };
    let request = Request::new::<CliRun>(RequestId::Number(9), &params).unwrap();
    assert!(request.params.as_ref().unwrap().get("stdin").is_none());
    assert_eq!(request.params::<CliRun>().unwrap(), params);
    let result = CliRunResult {
        exit_code: ExitCode::ConfirmationRequired,
        document: Some(json!({ "confirmationRequired": true })),
        text: None,
    };
    let response = Response::success::<CliRun>(RequestId::Number(9), &result).unwrap();
    let value = wire(&Message::Response(response.clone()));
    assert_eq!(value["result"]["exitCode"], 5);
    assert_eq!(response.result::<CliRun>().unwrap(), result);
}

#[test]
fn typed_params_reject_bad_shapes() {
    let raw = json!({ "jsonrpc": "2.0", "id": 4, "method": "rpc.call",
                      "params": { "method": "x", "caller": { "kind": "stranger" } } });
    let Message::Request(request) = Message::parse(&raw.to_string()).unwrap() else {
        panic!("expected a request");
    };
    assert_eq!(
        request.params::<RpcCall>().unwrap_err().code,
        error_code::INVALID_PARAMS
    );
    let raw = json!({ "jsonrpc": "2.0", "id": 5, "method": "rpc.call",
                      "params": { "method": "x", "caller": { "kind": "mcp" } } });
    let Message::Request(request) = Message::parse(&raw.to_string()).unwrap() else {
        panic!("expected a request");
    };
    let params = request.params::<RpcCall>().unwrap();
    assert_eq!(
        params.caller,
        Caller {
            kind: CallerKind::Mcp,
            extension: None
        }
    );
    assert_eq!(params.params, Value::Null);
}

#[test]
fn host_requests_use_closed_shell_services() {
    let raw = json!({ "jsonrpc": "2.0", "id": 6, "method": "host.shell.action",
                      "params": { "action": "pickColor" } });
    let Message::Request(request) = Message::parse(&raw.to_string()).unwrap() else {
        panic!("expected a request");
    };
    assert_eq!(
        request.params::<HostShellAction>().unwrap().action,
        ShellService::PickColor
    );
    let raw = json!({ "jsonrpc": "2.0", "id": 7, "method": "host.shell.action",
                      "params": { "action": "screenshot" } });
    let Message::Request(request) = Message::parse(&raw.to_string()).unwrap() else {
        panic!("expected a request");
    };
    assert!(request.params::<HostShellAction>().is_err());
}

#[test]
fn notifications_parse_by_method() {
    let emit = Notification::new::<EventEmit>(&TopicParams {
        topic: "media.nowPlaying@1".to_string(),
        payload: json!({ "title": "Song" }),
    })
    .unwrap();
    let Message::Notification(parsed) =
        Message::parse(&wire(&Message::Notification(emit)).to_string()).unwrap()
    else {
        panic!("expected a notification");
    };
    assert!(parsed.is::<EventEmit>());
    assert_eq!(
        parsed.params::<EventEmit>().unwrap().payload["title"],
        "Song"
    );

    let progress = Notification::new::<Progress>(&ProgressParams {
        message: Some("scanning".to_string()),
        fraction: Some(0.5),
    })
    .unwrap();
    assert_eq!(
        wire(&Message::Notification(progress)),
        json!({ "jsonrpc": "2.0", "method": "$/progress",
                "params": { "message": "scanning", "fraction": 0.5 } })
    );

    let cancel =
        Message::parse(r#"{"jsonrpc":"2.0","method":"$/cancel","params":{"id":"r1"}}"#).unwrap();
    let Message::Notification(cancel) = cancel else {
        panic!("expected a notification");
    };
    assert_eq!(
        cancel.params::<Cancel>().unwrap().id,
        RequestId::String("r1".to_string())
    );
}

#[test]
fn malformed_lines_map_to_standard_errors() {
    assert_eq!(
        Message::parse("{").unwrap_err().code,
        error_code::PARSE_ERROR
    );
    assert_eq!(
        Message::parse(r#"{"jsonrpc":"1.0","id":1,"method":"x"}"#)
            .unwrap_err()
            .code,
        error_code::INVALID_REQUEST
    );
    assert_eq!(
        Message::parse("[]").unwrap_err().code,
        error_code::INVALID_REQUEST
    );
    let oversized = format!("\"{}\"", "a".repeat(MAX_MESSAGE_BYTES));
    assert_eq!(
        Message::parse(&oversized).unwrap_err().code,
        error_code::INVALID_REQUEST
    );
}

#[test]
fn method_names_match_the_protocol() {
    assert_eq!(Initialize::NAME, "initialize");
    assert_eq!(CliRun::NAME, "cli.run");
    assert_eq!(HostShellAction::NAME, "host.shell.action");
    assert_eq!(
        <Cancel as veronica_protocol::rpc::NotificationMethod>::NAME,
        "$/cancel"
    );
}
