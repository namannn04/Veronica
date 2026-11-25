use serde::Deserialize;
use serde_json::Value;
use veronica_sdk::protocol::rpc::ErrorObject;
use veronica_sdk::rpc::{failure, params, reply};
use veronica_system::graftcode::{self, Connection};
use veronica_system::workspace::{self, DoctorRequest};

pub(crate) const METHODS: [&str; 3] = ["doctor", "discover", "call"];

#[derive(Deserialize)]
struct DoctorParams {
    request: DoctorRequest,
}

#[derive(Deserialize)]
struct DiscoverParams {
    connection: Connection,
}

#[derive(Deserialize)]
struct CallParams {
    connection: Connection,
    name: String,
    #[serde(default)]
    arguments: Value,
    #[serde(default)]
    confirmed: bool,
}

pub(crate) async fn call(method: &str, arguments: Value) -> Result<Value, ErrorObject> {
    let fail = |error: anyhow::Error| failure(&error);
    match method {
        "doctor" => {
            let DoctorParams { request } = params(arguments)?;
            reply(&workspace::doctor(request).await.map_err(fail)?)
        }
        "discover" => {
            let DiscoverParams { connection } = params(arguments)?;
            reply(&graftcode::discover(connection).await.map_err(fail)?)
        }
        "call" => {
            let CallParams {
                connection,
                name,
                arguments,
                confirmed,
            } = params(arguments)?;
            graftcode::call(connection, name, arguments, confirmed)
                .await
                .map_err(fail)
        }
        other => Err(ErrorObject::method_not_found(other)),
    }
}
