use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use jsonc_parser::cst::{CstInputValue, CstRootNode};
use jsonc_parser::ParseOptions;
use serde::Serialize;
use serde_json::Value;
use toml_edit::{value, Array, DocumentMut, Item, Table};

use super::file::Edit;
use super::{ServerEntry, SERVER_NAME};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Client {
    Claude,
    Codex,
    Cursor,
    Vscode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    One(Client),
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown client `{0}`; expected claude, codex, cursor, vscode or all")]
pub struct UnknownClient(String);

impl Client {
    pub const ALL: [Self; 4] = [Self::Claude, Self::Codex, Self::Cursor, Self::Vscode];

    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
            Self::Vscode => "vscode",
        }
    }

    pub fn config_file(self, homes: &Homes) -> PathBuf {
        match self {
            Self::Claude => homes.home.join(".claude.json"),
            Self::Codex => homes.codex_home().join("config.toml"),
            Self::Cursor => homes.home.join(".cursor").join("mcp.json"),
            Self::Vscode => homes.vscode_user().join("mcp.json"),
        }
    }

    pub fn is_installed(self, homes: &Homes) -> bool {
        let marker = match self {
            Self::Claude => homes.home.join(".claude"),
            Self::Codex => homes.codex_home(),
            Self::Cursor => homes.home.join(".cursor"),
            Self::Vscode => homes.vscode_user(),
        };
        present(&marker) || present(&self.config_file(homes))
    }

    pub(crate) fn format(self) -> Format {
        match self {
            Self::Claude | Self::Cursor => Format::Json("mcpServers"),
            Self::Vscode => Format::Json("servers"),
            Self::Codex => Format::Toml,
        }
    }
}

impl fmt::Display for Client {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl FromStr for Selection {
    type Err = UnknownClient;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text == "all" {
            return Ok(Self::All);
        }
        Client::ALL
            .into_iter()
            .find(|client| client.name() == text)
            .map(Self::One)
            .ok_or_else(|| UnknownClient(text.to_string()))
    }
}

impl Selection {
    pub fn clients(self) -> Vec<Client> {
        match self {
            Self::One(client) => vec![client],
            Self::All => Client::ALL.to_vec(),
        }
    }
}

fn present(path: &Path) -> bool {
    path.exists() || path.is_symlink()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Homes {
    pub home: PathBuf,
    pub codex_home: Option<PathBuf>,
    pub config_home: Option<PathBuf>,
}

impl Homes {
    pub fn current() -> Option<Self> {
        Self::from_env(|name| std::env::var_os(name))
    }

    pub fn from_env(var: impl Fn(&str) -> Option<OsString>) -> Option<Self> {
        let absolute = |name: &str| {
            var(name)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        Some(Self {
            home: absolute("HOME")?,
            codex_home: absolute("CODEX_HOME"),
            config_home: absolute("XDG_CONFIG_HOME"),
        })
    }

    fn codex_home(&self) -> PathBuf {
        self.codex_home
            .clone()
            .unwrap_or_else(|| self.home.join(".codex"))
    }

    fn vscode_user(&self) -> PathBuf {
        self.config_home
            .clone()
            .unwrap_or_else(|| self.home.join(".config"))
            .join("Code")
            .join("User")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Json(&'static str),
    Toml,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Registration {
    Absent,
    Matches,
    Differs,
}

impl Format {
    pub(crate) fn snippet(self, entry: &ServerEntry) -> String {
        match self {
            Self::Json(key) => {
                let document = serde_json::json!({ key: { SERVER_NAME: json_entry(entry) } });
                format!("{document:#}\n")
            }
            Self::Toml => {
                let mut document = DocumentMut::new();
                let mut servers = Table::new();
                servers.set_implicit(true);
                servers.insert(SERVER_NAME, Item::Table(toml_entry(entry)));
                document.insert("mcp_servers", Item::Table(servers));
                document.to_string()
            }
        }
    }

    pub(crate) fn install(self, text: Option<&str>, entry: &ServerEntry) -> Edit {
        match self {
            Self::Json(key) => json_install(text, key, entry),
            Self::Toml => toml_install(text, entry),
        }
    }

    pub(crate) fn uninstall(self, text: Option<&str>) -> Edit {
        match self {
            Self::Json(key) => json_uninstall(text, key),
            Self::Toml => toml_uninstall(text),
        }
    }

    pub(crate) fn registration(
        self,
        text: Option<&str>,
        entry: &ServerEntry,
    ) -> Result<Registration, String> {
        match self {
            Self::Json(key) => {
                let root = json_root(text)?;
                let existing =
                    json_servers(&root, key)?.and_then(|servers| servers.get(SERVER_NAME));
                Ok(classify(
                    existing.map(|existing| json_matches(existing, entry)),
                ))
            }
            Self::Toml => {
                let document = toml_document(text)?;
                let existing = toml_existing(&document)?;
                Ok(classify(
                    existing.map(|existing| toml_matches(existing, entry)),
                ))
            }
        }
    }
}

fn classify(matches: Option<bool>) -> Registration {
    match matches {
        None => Registration::Absent,
        Some(true) => Registration::Matches,
        Some(false) => Registration::Differs,
    }
}

fn json_entry(entry: &ServerEntry) -> Value {
    serde_json::json!({ "type": "stdio", "command": entry.command, "args": entry.args })
}

fn json_fields(entry: &ServerEntry) -> Vec<(String, CstInputValue)> {
    vec![
        ("type".to_string(), CstInputValue::from("stdio")),
        (
            "command".to_string(),
            CstInputValue::from(entry.command.as_str()),
        ),
        (
            "args".to_string(),
            CstInputValue::Array(entry.args.iter().map(|arg| arg.as_str().into()).collect()),
        ),
    ]
}

fn json_source(text: Option<&str>) -> &str {
    match text {
        Some(text) if !text.trim().is_empty() => text,
        _ => "{}\n",
    }
}

fn json_root(text: Option<&str>) -> Result<serde_json::Map<String, Value>, String> {
    match serde_json::from_str::<Value>(json_source(text)) {
        Ok(Value::Object(root)) => Ok(root),
        Ok(_) => Err("the top level is not a JSON object".to_string()),
        Err(error) => Err(format!(
            "not strict JSON ({error}); comments and trailing commas are not supported"
        )),
    }
}

fn json_servers<'a>(
    root: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<&'a serde_json::Map<String, Value>>, String> {
    match root.get(key) {
        None => Ok(None),
        Some(Value::Object(servers)) => Ok(Some(servers)),
        Some(_) => Err(format!("`{key}` is not a JSON object")),
    }
}

fn json_matches(existing: &Value, entry: &ServerEntry) -> bool {
    let args: Option<Vec<&str>> = existing
        .get("args")
        .and_then(Value::as_array)
        .map(|args| args.iter().filter_map(Value::as_str).collect());
    existing.get("type").and_then(Value::as_str) == Some("stdio")
        && existing.get("command").and_then(Value::as_str) == Some(entry.command.as_str())
        && args.is_some_and(|args| args == entry.args)
}

fn json_cst(source: &str) -> Result<CstRootNode, String> {
    CstRootNode::parse(source, &ParseOptions::default()).map_err(|error| error.to_string())
}

fn json_install(text: Option<&str>, key: &str, entry: &ServerEntry) -> Edit {
    let source = json_source(text);
    let existing = match json_root(text).and_then(|root| {
        json_servers(&root, key)
            .map(|servers| servers.and_then(|servers| servers.get(SERVER_NAME).cloned()))
    }) {
        Ok(existing) => existing,
        Err(reason) => return Edit::Refuse(reason),
    };
    if existing
        .as_ref()
        .is_some_and(|existing| json_matches(existing, entry))
    {
        return Edit::Keep;
    }
    let root = match json_cst(source) {
        Ok(root) => root,
        Err(reason) => return Edit::Refuse(reason),
    };
    let Some(servers) = root.object_value_or_set().object_value_or_create(key) else {
        return Edit::Refuse(format!("`{key}` is not a JSON object"));
    };
    match servers.get(SERVER_NAME) {
        Some(property) => match property.object_value() {
            Some(server) => {
                for (name, field) in json_fields(entry) {
                    match server.get(&name) {
                        Some(current) => current.set_value(field),
                        None => {
                            server.append(&name, field);
                        }
                    }
                }
            }
            None => property.set_value(CstInputValue::Object(json_fields(entry))),
        },
        None => {
            servers.append(SERVER_NAME, CstInputValue::Object(json_fields(entry)));
        }
    }
    Edit::Write {
        body: terminated(root.to_string()),
        replaced: existing.is_some(),
    }
}

fn json_uninstall(text: Option<&str>, key: &str) -> Edit {
    if text.is_none() {
        return Edit::Keep;
    }
    let present = match json_root(text).and_then(|root| {
        json_servers(&root, key)
            .map(|servers| servers.is_some_and(|servers| servers.contains_key(SERVER_NAME)))
    }) {
        Ok(present) => present,
        Err(reason) => return Edit::Refuse(reason),
    };
    if !present {
        return Edit::Keep;
    }
    let root = match json_cst(json_source(text)) {
        Ok(root) => root,
        Err(reason) => return Edit::Refuse(reason),
    };
    if let Some(property) = root
        .object_value()
        .and_then(|root| root.object_value(key))
        .and_then(|servers| servers.get(SERVER_NAME))
    {
        property.remove();
    }
    Edit::Write {
        body: terminated(root.to_string()),
        replaced: true,
    }
}

fn terminated(mut body: String) -> String {
    if !body.ends_with('\n') {
        body.push('\n');
    }
    body
}

fn toml_entry(entry: &ServerEntry) -> Table {
    let mut table = Table::new();
    table.insert("command", value(entry.command.as_str()));
    table.insert("args", value(toml_args(entry)));
    table
}

fn toml_args(entry: &ServerEntry) -> Array {
    entry.args.iter().map(String::as_str).collect()
}

fn toml_document(text: Option<&str>) -> Result<DocumentMut, String> {
    text.unwrap_or_default()
        .parse::<DocumentMut>()
        .map_err(|error| format!("not valid TOML: {}", error.to_string().trim()))
}

fn toml_existing(document: &DocumentMut) -> Result<Option<&Item>, String> {
    let Some(servers) = document.get("mcp_servers") else {
        return Ok(None);
    };
    let servers = servers
        .as_table_like()
        .ok_or_else(|| "`mcp_servers` is not a table".to_string())?;
    match servers.get(SERVER_NAME) {
        Some(existing) if !existing.is_table_like() => {
            Err(format!("`mcp_servers.{SERVER_NAME}` is not a table"))
        }
        existing => Ok(existing),
    }
}

fn toml_matches(existing: &Item, entry: &ServerEntry) -> bool {
    let args: Option<Vec<&str>> = existing
        .get("args")
        .and_then(Item::as_array)
        .map(|args| args.iter().filter_map(toml_edit::Value::as_str).collect());
    existing.get("command").and_then(Item::as_str) == Some(entry.command.as_str())
        && args.is_some_and(|args| args == entry.args)
}

fn toml_install(text: Option<&str>, entry: &ServerEntry) -> Edit {
    let mut document = match toml_document(text) {
        Ok(document) => document,
        Err(reason) => return Edit::Refuse(reason),
    };
    let replaced = match toml_existing(&document) {
        Ok(Some(existing)) if toml_matches(existing, entry) => return Edit::Keep,
        Ok(existing) => existing.is_some(),
        Err(reason) => return Edit::Refuse(reason),
    };
    if !document.contains_key("mcp_servers") {
        let mut servers = Table::new();
        servers.set_implicit(true);
        document.insert("mcp_servers", Item::Table(servers));
    }
    let Some(servers) = document["mcp_servers"].as_table_like_mut() else {
        return Edit::Refuse("`mcp_servers` is not a table".to_string());
    };
    match servers
        .get_mut(SERVER_NAME)
        .and_then(Item::as_table_like_mut)
    {
        Some(server) => {
            server.insert("command", value(entry.command.as_str()));
            server.insert("args", value(toml_args(entry)));
        }
        None => {
            servers.insert(SERVER_NAME, Item::Table(toml_entry(entry)));
        }
    }
    Edit::Write {
        body: document.to_string(),
        replaced,
    }
}

fn toml_uninstall(text: Option<&str>) -> Edit {
    if text.is_none() {
        return Edit::Keep;
    }
    let mut document = match toml_document(text) {
        Ok(document) => document,
        Err(reason) => return Edit::Refuse(reason),
    };
    match toml_existing(&document) {
        Ok(Some(_)) => {}
        Ok(None) => return Edit::Keep,
        Err(reason) => return Edit::Refuse(reason),
    }
    if let Some(servers) = document["mcp_servers"].as_table_like_mut() {
        servers.remove(SERVER_NAME);
    }
    Edit::Write {
        body: document.to_string(),
        replaced: true,
    }
}
