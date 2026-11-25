use serde::{Deserialize, Serialize};
use serde_json::Value;
use veronica_core::ClipboardHistory;
use veronica_sdk::paths::AppDirectories;
use veronica_sdk::protocol::rpc::{error_code, ErrorObject};
use veronica_sdk::rpc::{failure, params, reply};

pub(crate) const METHODS: [&str; 3] = ["list", "remove", "clear"];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClipRow {
    pub id: u64,
    pub preview: String,
    pub text: String,
    pub lines: usize,
    pub bytes: usize,
    pub count: u32,
    pub last_seen: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Query {
    query: String,
}

#[derive(Deserialize)]
struct Target {
    id: u64,
}

fn list(history: &ClipboardHistory, query: &str) -> Vec<ClipRow> {
    history
        .search(query)
        .into_iter()
        .map(|entry| ClipRow {
            id: entry.id,
            preview: entry.preview(),
            text: entry.text.clone(),
            lines: entry.line_count(),
            bytes: entry.byte_len(),
            count: entry.count,
            last_seen: entry.last_seen.to_rfc3339(),
        })
        .collect()
}

pub(crate) fn call(method: &str, arguments: Value) -> Result<Value, ErrorObject> {
    let directories = AppDirectories::current().map_err(|error| failure(&error))?;
    let path = directories.clipboard_db();
    let mut history = ClipboardHistory::load(&path).map_err(|error| failure(&error))?;
    match method {
        "list" => {
            let Query { query } = params(arguments)?;
            reply(&list(&history, &query))
        }
        "remove" => {
            let Target { id } = params(arguments)?;
            if !history.remove(id) {
                return Err(ErrorObject::new(
                    error_code::INVALID_PARAMS,
                    format!("no clipboard entry {id}"),
                ));
            }
            history.save(&path).map_err(|error| failure(&error))?;
            Ok(Value::Null)
        }
        "clear" => {
            history.clear();
            history.save(&path).map_err(|error| failure(&error))?;
            Ok(Value::Null)
        }
        other => Err(ErrorObject::method_not_found(other)),
    }
}
