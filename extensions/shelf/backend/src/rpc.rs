use serde::Deserialize;
use serde_json::{json, Value};
use veronica_sdk::protocol::rpc::ErrorObject;
use veronica_sdk::rpc::{failure, params};

use crate::cli::open;

pub(crate) const METHODS: [&str; 3] = ["list", "remove", "clear"];

#[derive(Deserialize)]
struct RemoveParams {
    id: String,
}

pub(crate) fn call(method: &str, arguments: Value) -> Result<Value, ErrorObject> {
    let fail = |error: anyhow::Error| failure(&error);
    let shelf = open().map_err(fail)?;
    match method {
        "list" => {}
        "remove" => {
            let RemoveParams { id } = params(arguments)?;
            shelf.remove(&id).map_err(fail)?;
        }
        "clear" => {
            for entry in shelf.entries().map_err(fail)? {
                shelf.remove(&entry.id).map_err(fail)?;
            }
        }
        other => return Err(ErrorObject::method_not_found(other)),
    }
    Ok(json!({ "entries": shelf.entries().map_err(fail)? }))
}
