use serde_json::{json, Value};
use veronica_core::AppDirectories;
use veronica_database::mcp;
use veronica_database::store::MetadataStore;

use super::protocol::{RpcError, INTERNAL_ERROR};

pub(crate) fn tools() -> Vec<Value> {
    match mcp::tools() {
        Value::Array(tools) => tools,
        _ => Vec::new(),
    }
}

pub(crate) fn owns(name: &str) -> bool {
    tools().iter().any(|tool| tool["name"] == name)
}

fn answer(name: &str, arguments: &Value) -> Value {
    let store = AppDirectories::current()
        .and_then(|directories| MetadataStore::open(directories.database_store()));
    match store {
        Ok(store) => mcp::call_tool(&store, name, arguments),
        Err(error) => json!({
            "content": [{ "type": "text", "text": json!({ "error": format!("{error:#}") }).to_string() }],
            "isError": true,
        }),
    }
}

pub(crate) async fn call(name: String, arguments: Value) -> Result<Value, RpcError> {
    tokio::task::spawn_blocking(move || answer(&name, &arguments))
        .await
        .map_err(|error| {
            RpcError::new(
                INTERNAL_ERROR,
                format!("the database tool stopped: {error}"),
            )
        })
}
