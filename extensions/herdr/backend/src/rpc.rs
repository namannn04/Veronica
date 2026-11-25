use serde::Deserialize;
use serde_json::Value;
use veronica_sdk::protocol::rpc::ErrorObject;
use veronica_sdk::rpc::{failure, params, reply};
use veronica_sdk::HostClient;

use crate::terminal::{attach, terminals};

pub(crate) const METHODS: [&str; 7] = [
    "board", "open", "attach", "output", "input", "resize", "stop",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenParams {
    session: String,
    #[serde(default)]
    pane_id: Option<String>,
}

#[derive(Deserialize)]
struct AttachParams {
    session: String,
    pane: String,
}

#[derive(Deserialize)]
struct OutputParams {
    id: String,
    #[serde(default)]
    cursor: u64,
}

#[derive(Deserialize)]
struct InputParams {
    id: String,
    bytes: String,
}

#[derive(Deserialize)]
struct ResizeParams {
    id: String,
    cols: u16,
    rows: u16,
}

#[derive(Deserialize)]
struct StopParams {
    id: String,
}

pub(crate) async fn call(
    host: HostClient,
    method: &str,
    arguments: Value,
) -> Result<Value, ErrorObject> {
    let fail = |error: anyhow::Error| failure(&error);
    match method {
        "board" => reply(&veronica_system::herdr::board().await.map_err(fail)?),
        "open" => {
            let OpenParams { session, pane_id } = params(arguments)?;
            veronica_system::herdr::open_terminal(&session, pane_id.as_deref()).map_err(fail)?;
            Ok(Value::Null)
        }
        "attach" => {
            let AttachParams { session, pane } = params(arguments)?;
            reply(&attach(host, session, pane).await.map_err(fail)?)
        }
        "output" => {
            let OutputParams { id, cursor } = params(arguments)?;
            reply(&terminals().output(&id, cursor).map_err(fail)?)
        }
        "input" => {
            let InputParams { id, bytes } = params(arguments)?;
            terminals().input(&id, bytes).map_err(fail)?;
            Ok(Value::Null)
        }
        "resize" => {
            let ResizeParams { id, cols, rows } = params(arguments)?;
            terminals().resize(&id, cols, rows).map_err(fail)?;
            Ok(Value::Null)
        }
        "stop" => {
            let StopParams { id } = params(arguments)?;
            terminals().stop(&id).map_err(fail)?;
            Ok(Value::Null)
        }
        other => Err(ErrorObject::method_not_found(other)),
    }
}
