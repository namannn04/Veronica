use serde::Deserialize;
use serde_json::Value;
use veronica_sdk::protocol::rpc::ErrorObject;
use veronica_sdk::rpc::{failure, params, reply};

pub(crate) const METHODS: [&str; 1] = ["site"];

#[derive(Deserialize)]
struct SiteParams {
    site: String,
    limit: usize,
    concurrency: usize,
}

pub(crate) async fn call(method: &str, arguments: Value) -> Result<Value, ErrorObject> {
    match method {
        "site" => {
            let SiteParams {
                site,
                limit,
                concurrency,
            } = params(arguments)?;
            let report = veronica_audit::audit(&site, limit, concurrency)
                .await
                .map_err(|error| failure(&error))?;
            reply(&report)
        }
        other => Err(ErrorObject::method_not_found(other)),
    }
}
