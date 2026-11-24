use std::sync::Arc;

use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use veronica_host::host_api::HostServices;
use veronica_host::Router;
use veronica_protocol::rpc::{Caller, CallerKind};
use veronica_protocol::{canonical, ExitCode};

use crate::format::{Failure, Output};
use crate::routes;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    method: String,
    #[serde(default)]
    params: Value,
}

pub(crate) async fn run(id: &str, request: &str, output: Output) -> Result<()> {
    let request: Request = serde_json::from_str(request).map_err(|error| {
        Failure::new(
            ExitCode::Usage,
            format!(
                "the request must look like {{\"method\": \"...\", \"params\": {{}}}}: {error}"
            ),
        )
    })?;
    let router = Router::oneshot(Arc::new(HostServices::current()?));
    let caller = Caller {
        kind: CallerKind::Cli,
        extension: None,
    };
    let value = routes::interruptible(router.invoke(id, &request.method, request.params, caller))
        .await
        .ok_or_else(|| Failure::new(ExitCode::Interrupted, "interrupted"))?
        .map_err(|error| routes::backend_failure(&error))?;
    output.emit(&value, || {
        canonical::value_to_string_pretty(&value)
            .trim_end()
            .to_string()
    })
}
