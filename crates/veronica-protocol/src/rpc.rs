use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::contract_id::ContractId;
use crate::exit::ExitCode;
use crate::manifest::{Permissions, ShellService};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
pub const JSONRPC_VERSION: &str = "2.0";

pub mod error_code {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
    pub const PERMISSION_DENIED: i64 = -32001;
    pub const UNAVAILABLE: i64 = -32002;
    pub const TIMEOUT: i64 = -32003;
    pub const VERSION_MISMATCH: i64 = -32004;
    pub const REQUEST_CANCELLED: i64 = -32800;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct JsonRpcVersion;

impl Serialize for JsonRpcVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(JSONRPC_VERSION)
    }
}

impl<'de> Deserialize<'de> for JsonRpcVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        if raw == JSONRPC_VERSION {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom(format!(
                "unsupported jsonrpc version {raw:?}"
            )))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: JsonRpcVersion,
    pub id: RequestId,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    pub jsonrpc: JsonRpcVersion,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: JsonRpcVersion,
    pub id: Option<RequestId>,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Result(Value),
    Error(ErrorObject),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message} ({code})")]
pub struct ErrorObject {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    Request(Request),
    Notification(Notification),
    Response(Response),
}

pub trait RequestMethod {
    const NAME: &'static str;
    type Params: Serialize + DeserializeOwned;
    type Result: Serialize + DeserializeOwned;
}

pub trait NotificationMethod {
    const NAME: &'static str;
    type Params: Serialize + DeserializeOwned;
}

fn encode_params<P: Serialize>(params: &P) -> serde_json::Result<Option<Value>> {
    let value = serde_json::to_value(params)?;
    Ok((!value.is_null()).then_some(value))
}

fn decode_params<P: DeserializeOwned>(params: Option<&Value>) -> Result<P, ErrorObject> {
    P::deserialize(params.unwrap_or(&Value::Null))
        .map_err(|error| ErrorObject::new(error_code::INVALID_PARAMS, error.to_string()))
}

impl ErrorObject {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    #[must_use]
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(
            error_code::METHOD_NOT_FOUND,
            format!("method {method:?} not found"),
        )
    }
}

impl Request {
    pub fn new<M: RequestMethod>(id: RequestId, params: &M::Params) -> serde_json::Result<Self> {
        Ok(Self {
            jsonrpc: JsonRpcVersion,
            id,
            method: M::NAME.to_string(),
            params: encode_params(params)?,
        })
    }

    pub fn is<M: RequestMethod>(&self) -> bool {
        self.method == M::NAME
    }

    pub fn params<M: RequestMethod>(&self) -> Result<M::Params, ErrorObject> {
        if !self.is::<M>() {
            return Err(ErrorObject::method_not_found(&self.method));
        }
        decode_params(self.params.as_ref())
    }
}

impl Notification {
    pub fn new<M: NotificationMethod>(params: &M::Params) -> serde_json::Result<Self> {
        Ok(Self {
            jsonrpc: JsonRpcVersion,
            method: M::NAME.to_string(),
            params: encode_params(params)?,
        })
    }

    pub fn is<M: NotificationMethod>(&self) -> bool {
        self.method == M::NAME
    }

    pub fn params<M: NotificationMethod>(&self) -> Result<M::Params, ErrorObject> {
        if !self.is::<M>() {
            return Err(ErrorObject::method_not_found(&self.method));
        }
        decode_params(self.params.as_ref())
    }
}

impl Response {
    pub fn success<M: RequestMethod>(
        id: RequestId,
        result: &M::Result,
    ) -> serde_json::Result<Self> {
        Ok(Self {
            jsonrpc: JsonRpcVersion,
            id: Some(id),
            outcome: Outcome::Result(serde_json::to_value(result)?),
        })
    }

    pub fn failure(id: Option<RequestId>, error: ErrorObject) -> Self {
        Self {
            jsonrpc: JsonRpcVersion,
            id,
            outcome: Outcome::Error(error),
        }
    }

    pub fn result<M: RequestMethod>(self) -> Result<M::Result, ErrorObject> {
        match self.outcome {
            Outcome::Result(value) => M::Result::deserialize(value)
                .map_err(|error| ErrorObject::new(error_code::INTERNAL_ERROR, error.to_string())),
            Outcome::Error(error) => Err(error),
        }
    }
}

impl Message {
    pub fn parse(line: &str) -> Result<Self, ErrorObject> {
        if line.len() > MAX_MESSAGE_BYTES {
            return Err(ErrorObject::new(
                error_code::INVALID_REQUEST,
                format!("message exceeds {MAX_MESSAGE_BYTES} bytes"),
            ));
        }
        let value: Value = serde_json::from_str(line)
            .map_err(|error| ErrorObject::new(error_code::PARSE_ERROR, error.to_string()))?;
        Self::deserialize(value).map_err(|_| {
            ErrorObject::new(error_code::INVALID_REQUEST, "not a JSON-RPC 2.0 message")
        })
    }

    pub fn to_line(&self) -> serde_json::Result<String> {
        let mut line = serde_json::to_string(self)?;
        line.push('\n');
        Ok(line)
    }
}

macro_rules! request_method {
    ($type:ident, $name:literal, $params:ty, $result:ty) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $type;

        impl RequestMethod for $type {
            const NAME: &'static str = $name;
            type Params = $params;
            type Result = $result;
        }
    };
}

macro_rules! notification_method {
    ($type:ident, $name:literal, $params:ty) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $type;

        impl NotificationMethod for $type {
            const NAME: &'static str = $name;
            type Params = $params;
        }
    };
}

request_method!(Initialize, "initialize", InitializeParams, InitializeResult);
request_method!(Shutdown, "shutdown", (), ());
request_method!(CliRun, "cli.run", CliRunParams, CliRunResult);
request_method!(RpcCall, "rpc.call", RpcCallParams, Value);
request_method!(
    ContractRequest,
    "contract.request",
    ContractRequestParams,
    Value
);
request_method!(SurfaceAction, "surface.action", SurfaceActionParams, ());
request_method!(ShortcutFired, "shortcut.fired", ShortcutFiredParams, ());
request_method!(HealthPing, "health.ping", (), ());
notification_method!(Initialized, "initialized", ());
notification_method!(HostTopic, "host.topic", TopicParams);
notification_method!(Cancel, "$/cancel", CancelParams);

request_method!(HostSettingsGet, "host.settings.get", KeyParams, ValueResult);
request_method!(HostSettingsSet, "host.settings.set", SettingsSetParams, ());
request_method!(HostNotify, "host.notify", NotifyParams, ());
request_method!(
    HostSurfaceUpdate,
    "host.surface.update",
    SurfaceUpdateParams,
    ()
);
request_method!(
    HostContractPublish,
    "host.contract.publish",
    ContractPublishParams,
    ()
);
request_method!(
    HostContractRequest,
    "host.contract.request",
    HostContractRequestParams,
    Value
);
request_method!(
    HostShellAction,
    "host.shell.action",
    ShellActionParams,
    Value
);
request_method!(HostSecretGet, "host.secret.get", KeyParams, SecretResult);
request_method!(HostSecretSet, "host.secret.set", SecretSetParams, ());
request_method!(HostOpen, "host.open", OpenParams, ());
request_method!(
    HostPromptSecret,
    "host.prompt.secret",
    PromptSecretParams,
    SecretResult
);
request_method!(HostLog, "host.log", LogParams, ());
notification_method!(EventEmit, "event.emit", TopicParams);
notification_method!(Progress, "$/progress", ProgressParams);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunMode {
    Oneshot,
    Service,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol: u32,
    pub host: HostInfo,
    pub extension: ExtensionInfo,
    pub mode: RunMode,
    pub dirs: ExtensionDirs,
    pub granted: Permissions,
    pub session: SessionInfo,
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    pub version: String,
    pub api_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionInfo {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionDirs {
    pub install: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
    pub state: PathBuf,
    pub runtime: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desktop: Option<String>,
    pub packaging: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol: u32,
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliRunParams {
    pub argv: Vec<String>,
    pub json: bool,
    pub confirm: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,
    pub cwd: PathBuf,
    pub tty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliRunResult {
    pub exit_code: ExitCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CallerKind {
    Host,
    Ui,
    Cli,
    Mcp,
    Surface,
    Extension,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caller {
    pub kind: CallerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcCallParams {
    pub method: String,
    #[serde(default)]
    pub params: Value,
    pub caller: Caller,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractRequestParams {
    pub contract: ContractId,
    #[serde(default)]
    pub params: Value,
    pub caller: Caller,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceActionParams {
    pub surface: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutFiredParams {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicParams {
    pub topic: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelParams {
    pub id: RequestId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyParams {
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueResult {
    #[serde(default)]
    pub value: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSetParams {
    pub key: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyParams {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceUpdateParams {
    pub surface: String,
    pub state: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractPublishParams {
    pub contract: ContractId,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostContractRequestParams {
    pub contract: ContractId,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellActionParams {
    pub action: ShellService,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretSetParams {
    pub key: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretResult {
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenParams {
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptSecretParams {
    pub prompt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogParams {
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fraction: Option<f64>,
}
