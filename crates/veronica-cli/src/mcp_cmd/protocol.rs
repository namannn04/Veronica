use serde_json::{json, Value};

pub(crate) const PARSE_ERROR: i64 = -32700;
pub(crate) const INVALID_REQUEST: i64 = -32600;
pub(crate) const METHOD_NOT_FOUND: i64 = -32601;
pub(crate) const INVALID_PARAMS: i64 = -32602;
pub(crate) const INTERNAL_ERROR: i64 = -32603;
pub(crate) const RESOURCE_NOT_FOUND: i64 = -32002;

pub(crate) const LATEST_VERSION: &str = "2025-11-25";
pub(crate) const SUPPORTED_VERSIONS: [&str; 4] =
    ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RpcError {
    pub(crate) code: i64,
    pub(crate) message: String,
    pub(crate) data: Option<Value>,
}

impl RpcError {
    pub(crate) fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub(crate) fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

pub(crate) fn negotiate(requested: Option<&str>) -> &'static str {
    SUPPORTED_VERSIONS
        .into_iter()
        .find(|version| Some(*version) == requested)
        .unwrap_or(LATEST_VERSION)
}

pub(crate) fn success(id: &Value, result: Value) -> Value {
    let mut message = json!({ "jsonrpc": "2.0", "id": id });
    message["result"] = result;
    message
}

pub(crate) fn failure(id: &Value, error: &RpcError) -> Value {
    let mut body = json!({ "code": error.code, "message": error.message });
    if let Some(data) = &error.data {
        body["data"] = data.clone();
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": body })
}

pub(crate) fn notification(method: &str) -> Value {
    json!({ "jsonrpc": "2.0", "method": method })
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Message {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
    },
    Response,
    Invalid {
        id: Value,
        error: RpcError,
    },
}

pub(crate) fn parse(line: &str) -> Message {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => {
            return Message::Invalid {
                id: Value::Null,
                error: RpcError::new(PARSE_ERROR, format!("not JSON: {error}")),
            }
        }
    };
    let invalid = |id: Value, message: &str| Message::Invalid {
        id,
        error: RpcError::new(INVALID_REQUEST, message),
    };
    let Value::Object(object) = value else {
        return invalid(Value::Null, "a message must be one JSON object");
    };
    let id = object.get("id").cloned();
    let usable_id = id
        .clone()
        .filter(|id| id.is_string() || id.is_number())
        .unwrap_or(Value::Null);
    if object.get("jsonrpc") != Some(&json!("2.0")) {
        return invalid(usable_id, "jsonrpc must be \"2.0\"");
    }
    let Some(method) = object.get("method") else {
        if object.contains_key("result") || object.contains_key("error") {
            return Message::Response;
        }
        return invalid(usable_id, "a request needs a method");
    };
    let Some(method) = method.as_str().map(str::to_string) else {
        return invalid(usable_id, "method must be a string");
    };
    let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
    if !params.is_object() && !params.is_array() {
        return invalid(usable_id, "params must be an object");
    }
    match id {
        None => Message::Notification { method },
        Some(id) if id.is_string() || id.is_number() => Message::Request { id, method, params },
        Some(_) => invalid(Value::Null, "id must be a string or a number"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_echoed_only_when_supported() {
        for version in SUPPORTED_VERSIONS {
            assert_eq!(negotiate(Some(version)), version);
        }
        assert_eq!(negotiate(Some("1999-01-01")), LATEST_VERSION);
        assert_eq!(negotiate(None), LATEST_VERSION);
    }

    #[test]
    fn malformed_messages_get_the_right_codes() {
        let code = |line: &str| match parse(line) {
            Message::Invalid { error, .. } => error.code,
            other => panic!("{other:?}"),
        };
        assert_eq!(code("{"), PARSE_ERROR);
        assert_eq!(code("[]"), INVALID_REQUEST);
        assert_eq!(code(r#"{"id":1,"method":"ping"}"#), INVALID_REQUEST);
        assert_eq!(
            code(r#"{"jsonrpc":"2.0","id":1,"method":5}"#),
            INVALID_REQUEST
        );
        assert_eq!(
            code(r#"{"jsonrpc":"2.0","id":{},"method":"ping"}"#),
            INVALID_REQUEST
        );
        assert_eq!(
            parse(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
            Message::Notification {
                method: "notifications/initialized".to_string()
            }
        );
        assert_eq!(
            parse(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#),
            Message::Response
        );
    }
}
