use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use veronica_host::connection::BackendError;
use veronica_host::host_api::Grant;
use veronica_host::install::HOST_API;
use veronica_host::permissions::settings_namespace;
use veronica_protocol::manifest::HostPermission;
use veronica_protocol::rpc::{
    error_code, Caller, CallerKind, ErrorObject, HostSettingsGet, HostSettingsSet, JsonRpcVersion,
    Request, RequestId, RequestMethod,
};
use veronica_protocol::{ContractId, Manifest};

use super::files::granted_file;
use super::host::{Host, INTERFACE};
use super::protocol::SCHEME;
use super::storage::{QuotaExceeded, MAX_KEY_BYTES, QUOTA_BYTES};

const CALL_TIMEOUT: Duration = Duration::from_mins(1);
const LONGEST_CALL: Duration = Duration::from_mins(10);
const CONFIRM_PATIENCE: Duration = Duration::from_mins(5);
const MAX_CLIPBOARD_BYTES: usize = 1024 * 1024;
const MAX_URL_BYTES: usize = 2048;
const TOAST_KINDS: [&str; 4] = ["info", "success", "warning", "error"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) enum Code {
    #[serde(rename = "E_PERMISSION")]
    Permission,
    #[serde(rename = "E_UNAVAILABLE")]
    Unavailable,
    #[serde(rename = "E_TIMEOUT")]
    Timeout,
    #[serde(rename = "E_INVALID")]
    Invalid,
    #[serde(rename = "E_BACKEND")]
    Backend,
    #[serde(rename = "E_VERSION")]
    Version,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct Failure {
    pub code: Code,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl Failure {
    fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self::new(Code::Invalid, message)
    }

    fn unavailable(message: impl std::fmt::Display) -> Self {
        Self::new(Code::Unavailable, format!("{message:#}"))
    }

    fn permission(extension: &str, permission: &str) -> Self {
        Self {
            code: Code::Permission,
            message: format!("{extension} was not granted {permission}"),
            data: Some(json!({ "permission": permission })),
        }
    }
}

impl From<ErrorObject> for Failure {
    fn from(error: ErrorObject) -> Self {
        let code = match error.code {
            error_code::PERMISSION_DENIED => Code::Permission,
            error_code::UNAVAILABLE => Code::Unavailable,
            error_code::TIMEOUT => Code::Timeout,
            error_code::INVALID_PARAMS | error_code::METHOD_NOT_FOUND => Code::Invalid,
            error_code::VERSION_MISMATCH => Code::Version,
            _ => Code::Backend,
        };
        Self {
            code,
            message: error.message,
            data: error.data,
        }
    }
}

impl From<BackendError> for Failure {
    fn from(error: BackendError) -> Self {
        let code = match error {
            BackendError::Rpc(error) => return error.into(),
            BackendError::NotInstalled(_)
            | BackendError::Disabled(_)
            | BackendError::NoBackend(_)
            | BackendError::CrashLooping(_) => Code::Unavailable,
            BackendError::Timeout(_) => Code::Timeout,
            BackendError::Launch(_) | BackendError::Exited(_) | BackendError::Protocol(_) => {
                Code::Backend
            }
        };
        Self::new(code, error.to_string())
    }
}

type Reply = Result<Value, Failure>;

fn text<'a>(params: &'a Value, key: &str) -> Result<&'a str, Failure> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Failure::invalid(format!("{key} must be text")))
}

fn short<'a>(params: &'a Value, key: &str, limit: usize) -> Result<&'a str, Failure> {
    let value = text(params, key)?;
    if value.is_empty() || value.chars().count() > limit {
        return Err(Failure::invalid(format!(
            "{key} must be 1 to {limit} characters"
        )));
    }
    Ok(value)
}

fn optional<'a>(params: &'a Value, key: &str, limit: usize) -> Result<Option<&'a str>, Failure> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => short(params, key, limit).map(Some),
    }
}

fn argument(params: &Value) -> Value {
    params.get("params").cloned().unwrap_or(Value::Null)
}

fn patience(params: &Value) -> Result<Duration, Failure> {
    match params.get("timeoutMs") {
        None | Some(Value::Null) => Ok(CALL_TIMEOUT),
        Some(value) => value
            .as_u64()
            .map(|millis| Duration::from_millis(millis).min(LONGEST_CALL))
            .ok_or_else(|| Failure::invalid("timeoutMs must be a whole number")),
    }
}

fn settings_key(params: &Value) -> Result<&str, Failure> {
    let key = short(params, "key", 128)?;
    let plain = key
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'));
    if plain {
        Ok(key)
    } else {
        Err(Failure::invalid(
            "key may only hold letters, digits, dots, dashes and underscores",
        ))
    }
}

fn storage_failure(error: &anyhow::Error) -> Failure {
    match error.downcast_ref::<QuotaExceeded>() {
        Some(quota) => Failure {
            code: Code::Invalid,
            message: quota.to_string(),
            data: Some(json!({ "quota": QUOTA_BYTES, "needed": quota.needed })),
        },
        None => Failure::unavailable(error),
    }
}

struct Frame<'a> {
    host: &'a Host,
    id: &'a str,
    page: &'a str,
    manifest: Manifest,
}

pub(crate) async fn dispatch(
    host: &Host,
    extension: &str,
    page: &str,
    method: &str,
    params: Value,
) -> Reply {
    let frame = Frame::open(host, extension, page)?;
    match method {
        "backend.call" => frame.backend_call(&params).await,
        "backend.subscribe" => Ok(frame.subscribe(short(&params, "topic", 128)?, true)),
        "backend.unsubscribe" => Ok(frame.subscribe(short(&params, "topic", 128)?, false)),
        "storage.get" | "storage.set" | "storage.remove" | "storage.keys" => {
            frame.storage(method, &params)
        }
        "settings.get" | "settings.set" => frame.settings(method, &params),
        "contracts.request" => frame.contract_request(&params).await,
        "contracts.subscribe" => {
            let contract = frame.used_contract(&params)?;
            Ok(frame.subscribe(&format!("contract:{contract}"), true))
        }
        "nav.open" | "nav.setBadge" | "nav.setTitle" => frame.navigate(method, &params),
        "ui.toast" => frame.toast(&params),
        "ui.confirm" => frame.confirm(&params).await,
        "ui.openExternal" => frame.open_external(&params),
        "clipboard.writeText" => frame.copy(&params).await,
        "files.url" => frame.file_url(&params),
        other => Err(Failure::invalid(format!("unknown method {other}"))),
    }
}

impl<'a> Frame<'a> {
    fn open(host: &'a Host, id: &'a str, page: &'a str) -> Result<Self, Failure> {
        let installed = host.installed(id)?;
        if !installed.entry.enabled {
            return Err(BackendError::Disabled(id.to_string()).into());
        }
        let manifest = installed.manifest;
        if manifest.engines.host_api > HOST_API {
            return Err(Failure::new(
                Code::Version,
                format!(
                    "{id} needs host API {}, this app provides {HOST_API}",
                    manifest.engines.host_api
                ),
            ));
        }
        let frame = Self {
            host,
            id,
            page,
            manifest,
        };
        frame.own_page(page)?;
        Ok(frame)
    }

    fn own_page(&self, page: &str) -> Result<(), Failure> {
        let declared = self
            .manifest
            .ui
            .iter()
            .flat_map(|ui| &ui.pages)
            .any(|declared| declared.id == page);
        if declared {
            Ok(())
        } else {
            Err(Failure::invalid(format!("{} has no page {page}", self.id)))
        }
    }

    fn require(&self, permission: HostPermission, name: &str) -> Result<(), Failure> {
        if self.manifest.permissions.host.contains(&permission) {
            Ok(())
        } else {
            Err(Failure::permission(self.id, name))
        }
    }

    fn caller(&self) -> Caller {
        Caller {
            kind: CallerKind::Ui,
            extension: Some(self.id.to_string()),
        }
    }

    fn tell_shell(&self, mut payload: Value) -> Value {
        payload["extension"] = json!(self.id);
        payload["page"] = json!(self.page);
        self.host.emit(INTERFACE, payload);
        Value::Null
    }

    async fn backend_call(&self, params: &Value) -> Reply {
        let method = short(params, "method", 128)?;
        let limit = patience(params)?;
        let call = self
            .host
            .router()
            .invoke(self.id, method, argument(params), self.caller());
        match tokio::time::timeout(limit, call).await {
            Ok(answer) => Ok(answer?),
            Err(_) => Err(BackendError::Timeout(limit).into()),
        }
    }

    fn subscribe(&self, topic: &str, wanted: bool) -> Value {
        self.host.subscribe(self.id, self.page, topic, wanted);
        Value::Null
    }

    fn storage(&self, method: &str, params: &Value) -> Reply {
        let key = if method == "storage.keys" {
            ""
        } else {
            let key = text(params, "key")?;
            if key.is_empty() || key.len() > MAX_KEY_BYTES {
                return Err(Failure::invalid(format!(
                    "key must be 1 to {MAX_KEY_BYTES} bytes"
                )));
            }
            key
        };
        let value = match (method, params.get("value")) {
            ("storage.set", None) => return Err(Failure::invalid("value is required")),
            (_, value) => value.cloned().unwrap_or(Value::Null),
        };
        self.host
            .with_storage(self.id, |storage| {
                Ok(match method {
                    "storage.keys" => json!(storage.keys()),
                    "storage.get" => storage.get(key).cloned().unwrap_or(Value::Null),
                    "storage.set" => {
                        storage.set(key, value)?;
                        Value::Null
                    }
                    _ => json!(storage.remove(key)?),
                })
            })
            .map_err(|error| storage_failure(&error))
    }

    fn settings(&self, method: &str, params: &Value) -> Reply {
        let key = format!("{}{}", settings_namespace(self.id), settings_key(params)?);
        let (name, arguments) = if method == "settings.get" {
            (HostSettingsGet::NAME, json!({ "key": key }))
        } else {
            let value = params.get("value").cloned().unwrap_or(Value::Null);
            (HostSettingsSet::NAME, json!({ "key": key, "value": value }))
        };
        let request = Request {
            jsonrpc: JsonRpcVersion,
            id: RequestId::Number(0),
            method: name.to_string(),
            params: Some(arguments),
        };
        let grant = Grant {
            id: self.id.to_string(),
            permissions: self.manifest.permissions.clone(),
        };
        let answer = self.host.services().handle(&grant, &request)?;
        Ok(answer.get("value").cloned().unwrap_or(Value::Null))
    }

    fn used_contract(&self, params: &Value) -> Result<ContractId, Failure> {
        let contract: ContractId = text(params, "id")?.parse().map_err(
            |error: veronica_protocol::contract_id::ContractIdError| {
                Failure::invalid(error.to_string())
            },
        )?;
        if self.manifest.contracts.uses.contains(&contract) {
            Ok(contract)
        } else {
            Err(Failure::permission(self.id, &contract.to_string()))
        }
    }

    fn provider(&self, contract: &ContractId) -> Result<String, Failure> {
        let mut other_versions = Vec::new();
        for (id, entry) in &self.host.registry()?.extensions {
            let Some(manifest) = entry.enabled.then(|| self.host.manifest(id).ok()).flatten()
            else {
                continue;
            };
            for provided in &manifest.contracts.provides {
                if provided.id == *contract {
                    return Ok(id.clone());
                }
                if provided.id.name() == contract.name() {
                    other_versions.push(provided.id.version());
                }
            }
        }
        match other_versions.first() {
            Some(version) => Err(Failure::new(
                Code::Version,
                format!(
                    "{} is provided at version {version}, not {}",
                    contract.name(),
                    contract.version()
                ),
            )),
            None => Err(Failure::unavailable(format!(
                "no enabled extension provides {contract}"
            ))),
        }
    }

    async fn contract_request(&self, params: &Value) -> Reply {
        let contract = self.used_contract(params)?;
        let provider = self.provider(&contract)?;
        let limit = patience(params)?;
        let call = self.host.router().contract_request(
            &provider,
            contract,
            argument(params),
            self.caller(),
        );
        match tokio::time::timeout(limit, call).await {
            Ok(answer) => Ok(answer?),
            Err(_) => Err(BackendError::Timeout(limit).into()),
        }
    }

    fn navigate(&self, method: &str, params: &Value) -> Reply {
        let payload = match method {
            "nav.open" => {
                let target = text(params, "page")?;
                self.own_page(target)?;
                json!({ "action": "open", "target": target, "params": argument(params) })
            }
            "nav.setBadge" => {
                let badge = match params.get("badge") {
                    None | Some(Value::Null) => Value::Null,
                    Some(Value::Number(count)) if count.is_u64() => json!(count),
                    Some(Value::String(_)) => json!(short(params, "badge", 8)?),
                    Some(_) => return Err(Failure::invalid("badge must be text or a count")),
                };
                json!({ "action": "badge", "badge": badge })
            }
            _ => json!({ "action": "title", "title": short(params, "title", 80)? }),
        };
        Ok(self.tell_shell(payload))
    }

    fn toast(&self, params: &Value) -> Reply {
        let kind = optional(params, "kind", 16)?.unwrap_or("info");
        if !TOAST_KINDS.contains(&kind) {
            return Err(Failure::invalid(format!(
                "kind must be one of {TOAST_KINDS:?}"
            )));
        }
        let message = short(params, "message", 300)?;
        Ok(self.tell_shell(json!({ "action": "toast", "kind": kind, "message": message })))
    }

    async fn confirm(&self, params: &Value) -> Reply {
        self.host.shell()?;
        let payload = json!({
            "action": "confirm",
            "extension": self.id,
            "page": self.page,
            "title": short(params, "title", 120)?,
            "body": optional(params, "body", 600)?,
            "confirmLabel": optional(params, "confirmLabel", 40)?,
            "cancelLabel": optional(params, "cancelLabel", 40)?,
            "destructive": params.get("destructive").and_then(Value::as_bool).unwrap_or(false),
        });
        match self.host.confirm(payload, CONFIRM_PATIENCE).await {
            Some(accepted) => Ok(json!(accepted)),
            None => Err(BackendError::Timeout(CONFIRM_PATIENCE).into()),
        }
    }

    fn open_external(&self, params: &Value) -> Reply {
        self.require(HostPermission::OpenExternal, "openExternal")?;
        let url = text(params, "url")?;
        let safe = url.len() <= MAX_URL_BYTES
            && url.strip_prefix("https://").is_some_and(|rest| {
                rest.starts_with(|first: char| first.is_ascii_alphanumeric())
                    && rest.chars().all(|character| character.is_ascii_graphic())
            });
        if !safe {
            return Err(Failure::invalid("only https links can be opened"));
        }
        self.host
            .shell()?
            .open_external(url)
            .map_err(Failure::unavailable)?;
        Ok(Value::Null)
    }

    async fn copy(&self, params: &Value) -> Reply {
        self.require(HostPermission::ClipboardWrite, "clipboard:write")?;
        let text = text(params, "text")?;
        if text.len() > MAX_CLIPBOARD_BYTES {
            return Err(Failure::invalid(format!(
                "text is limited to {MAX_CLIPBOARD_BYTES} bytes"
            )));
        }
        self.host
            .shell()?
            .copy(text.to_string())
            .await
            .map_err(Failure::unavailable)?;
        Ok(Value::Null)
    }

    fn file_url(&self, params: &Value) -> Reply {
        let path = text(params, "path")?;
        let file = granted_file(&self.manifest.permissions, self.host.home(), path)
            .ok_or_else(|| Failure::permission(self.id, "a filesystem scope holding this file"))?;
        let token = self.host.files().issue(self.id, file);
        Ok(json!(format!("{SCHEME}://{}/_fs/{token}", self.id)))
    }
}
