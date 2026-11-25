use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use veronica_protocol::rpc::{error_code, ErrorObject};

pub fn params<T: DeserializeOwned>(params: Value) -> Result<T, ErrorObject> {
    serde_json::from_value(params)
        .map_err(|error| ErrorObject::new(error_code::INVALID_PARAMS, error.to_string()))
}

pub fn failure(error: &anyhow::Error) -> ErrorObject {
    ErrorObject::new(error_code::INTERNAL_ERROR, format!("{error:#}"))
}

pub fn reply<T: Serialize>(value: &T) -> Result<Value, ErrorObject> {
    serde_json::to_value(value)
        .map_err(|error| ErrorObject::new(error_code::INTERNAL_ERROR, error.to_string()))
}
