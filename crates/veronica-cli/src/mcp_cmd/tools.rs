use std::collections::BTreeSet;
use std::path::Path;
use std::process::Stdio;

use clap::CommandFactory as _;
use serde_json::{json, Map, Value};
use veronica_protocol::manifest::{Arg, ArgKind, ArgType, Effect};
use veronica_protocol::ExitCode;

use super::protocol::{RpcError, INVALID_PARAMS};
use crate::guide_cmd;
use crate::routes::{self, Routes};

pub(crate) const CATALOG_TOOL: &str = "veronica_catalog";
const STRUCTURED_SINCE: &str = "2025-06-18";
const REFUSED: [&str; 3] = ["--yes", "-y", "--json"];
const CATALOG_ACTIONS: [&str; 8] = [
    "list", "search", "info", "install", "update", "enable", "disable", "remove",
];
const CATALOG_STATUSES: [&str; 2] = ["enabled", "disabled"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tool {
    pub(crate) name: String,
    pub(crate) path: Vec<String>,
    pub(crate) about: Option<String>,
    pub(crate) effect: Effect,
    pub(crate) args: Vec<Arg>,
}

pub(crate) fn current_routes() -> Routes {
    let mut routes = Routes::discover();
    let _ = routes.command(crate::Cli::command());
    routes
}

pub(crate) fn discover(routes: &Routes) -> Vec<Tool> {
    let mut seen = BTreeSet::from([CATALOG_TOOL.to_string()]);
    guide_cmd::catalog(routes)
        .into_iter()
        .filter_map(|leaf| {
            let name = leaf.mcp_tool?;
            if !seen.insert(name.clone()) {
                eprintln!("vr: the MCP tool {name} is defined twice; keeping the first");
                return None;
            }
            Some(Tool {
                name,
                path: leaf.path.split(' ').map(str::to_string).collect(),
                about: leaf.about,
                effect: leaf.effect.unwrap_or(Effect::Destructive),
                args: leaf.args,
            })
        })
        .collect()
}

fn usage(tool: &Tool) -> String {
    let mut words = vec!["vr".to_string()];
    words.extend(tool.path.iter().cloned());
    for arg in &tool.args {
        let value = if arg.values.is_empty() {
            format!("<{}>", type_name(arg))
        } else {
            format!("<{}>", arg.values.join("|"))
        };
        let long = arg.long.clone().unwrap_or_else(|| arg.name.clone());
        let mut word = match arg.kind {
            ArgKind::Positional => format!("<{}>", arg.name),
            ArgKind::Option => format!("--{long} {value}"),
            ArgKind::Flag => format!("--{long}"),
        };
        if arg.multiple {
            word.push_str("...");
        }
        if !arg.required {
            word = format!("[{word}]");
        }
        words.push(word);
    }
    words.join(" ")
}

fn type_name(arg: &Arg) -> &'static str {
    match arg.value_type {
        ArgType::String => "string",
        ArgType::Integer => "integer",
        ArgType::Number => "number",
        ArgType::Boolean => "boolean",
        ArgType::Path => "path",
    }
}

fn annotations(title: &str, effect: Effect) -> Value {
    match effect {
        Effect::Read => json!({
            "title": title,
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
        }),
        Effect::Write => json!({
            "title": title,
            "readOnlyHint": false,
            "destructiveHint": false,
        }),
        Effect::Destructive => json!({
            "title": title,
            "readOnlyHint": false,
            "destructiveHint": true,
            "idempotentHint": false,
        }),
    }
}

fn confirm_property() -> Value {
    json!({
        "type": "boolean",
        "description": "Carry out the change. Without it the command changes nothing and returns a preview to show the person first."
    })
}

impl Tool {
    pub(crate) fn describe(&self) -> Value {
        let usage = usage(self);
        let description = match &self.about {
            Some(about) => format!("{about}\n\nUsage: {usage}"),
            None => format!("Usage: {usage}"),
        };
        let mut properties = Map::new();
        properties.insert(
            "arguments".to_string(),
            json!({
                "type": "array",
                "items": { "type": "string" },
                "description": "The words that follow the command, exactly as on the command line, without --json or --yes."
            }),
        );
        if self.effect == Effect::Destructive {
            properties.insert("confirm".to_string(), confirm_property());
        }
        let mut schema = json!({
            "type": "object",
            "properties": properties,
            "additionalProperties": false,
        });
        if self.args.iter().any(|arg| arg.required) {
            schema["required"] = json!(["arguments"]);
        }
        json!({
            "name": self.name,
            "description": description,
            "inputSchema": schema,
            "annotations": annotations(&format!("vr {}", self.path.join(" ")), self.effect),
        })
    }

    pub(crate) fn argv(&self, input: &Map<String, Value>) -> Result<Invocation, RpcError> {
        let destructive = self.effect == Effect::Destructive;
        for key in input.keys() {
            if key != "arguments" && !(destructive && key == "confirm") {
                return Err(RpcError::new(
                    INVALID_PARAMS,
                    format!("{} does not take {key:?}", self.name),
                ));
            }
        }
        let confirm = confirm_flag(input)?;
        let words = arguments(input)?;
        let mut argv = self.path.clone();
        argv.extend(words);
        Ok(Invocation { argv, confirm })
    }
}

fn confirm_flag(input: &Map<String, Value>) -> Result<bool, RpcError> {
    match input.get("confirm") {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(confirm)) => Ok(*confirm),
        Some(_) => Err(RpcError::new(INVALID_PARAMS, "confirm must be a boolean")),
    }
}

fn arguments(input: &Map<String, Value>) -> Result<Vec<String>, RpcError> {
    let words: Vec<String> = match input.get("arguments") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str().map(str::to_string))
            .collect::<Option<_>>()
            .ok_or_else(|| RpcError::new(INVALID_PARAMS, "arguments must be strings"))?,
        Some(_) => {
            return Err(RpcError::new(
                INVALID_PARAMS,
                "arguments must be an array of strings",
            ))
        }
    };
    if let Some(word) = words
        .iter()
        .take_while(|word| *word != "--")
        .find(|word| refused(word))
    {
        return Err(RpcError::new(
            INVALID_PARAMS,
            format!("{word} is not allowed in arguments; JSON output is always on and confirm replaces --yes"),
        ));
    }
    Ok(words)
}

fn refused(word: &str) -> bool {
    if REFUSED.contains(&word) || word.starts_with("--yes=") || word.starts_with("--json=") {
        return true;
    }
    word.strip_prefix('-').is_some_and(|cluster| {
        !cluster.starts_with('-')
            && cluster.contains('y')
            && cluster.chars().all(|c| c.is_ascii_alphabetic())
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Invocation {
    pub(crate) argv: Vec<String>,
    pub(crate) confirm: bool,
}

pub(crate) struct Completed {
    pub(crate) code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

impl Invocation {
    pub(crate) async fn run(&self, program: &Path) -> std::io::Result<Completed> {
        let mut command = tokio::process::Command::new(program);
        command.arg("--json");
        if self.confirm {
            command.arg("--yes");
        }
        let output = command
            .args(&self.argv)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output()
            .await?;
        Ok(Completed {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    pub(crate) fn rerun(&self) -> String {
        routes::rerun(&self.argv)
    }
}

pub(crate) fn result(completed: &Completed, version: &str) -> Value {
    let document: Value = serde_json::from_str(&completed.stdout).unwrap_or(Value::Null);
    let code = completed.code.and_then(ExitCode::from_code);
    let shown =
        code.is_some_and(|code| code.is_success() || code == ExitCode::ConfirmationRequired);
    let text = if shown || completed.stderr.trim().is_empty() {
        document_text(&document, completed)
    } else {
        completed.stderr.trim_end().to_string()
    };
    let mut reply = json!({
        "content": [{ "type": "text", "text": text }],
        "isError": !shown,
    });
    if document.is_object() && version >= STRUCTURED_SINCE {
        reply["structuredContent"] = document;
    }
    reply
}

pub(crate) fn document_result(document: &Value, version: &str) -> Value {
    let completed = Completed {
        code: Some(0),
        stdout: document.to_string(),
        stderr: String::new(),
    };
    result(&completed, version)
}

fn document_text(document: &Value, completed: &Completed) -> String {
    if document.is_null() && !completed.stdout.trim().is_empty() {
        return completed.stdout.trim_end().to_string();
    }
    veronica_protocol::canonical::value_to_string_pretty(document)
        .trim_end()
        .to_string()
}

pub(crate) fn catalog_tool() -> Value {
    json!({
        "name": CATALOG_TOOL,
        "description": "Find, inspect and manage Veronica extensions. list shows what is installed (filter with status), search looks through the catalog, info describes one extension. install, update, enable, disable and remove change this computer: without confirm they return a preview to show the person first. Installing or enabling an extension adds its tools to this server.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": CATALOG_ACTIONS },
                "id": { "type": "string", "description": "The extension id, for info, install, update, enable, disable and remove." },
                "query": { "type": "string", "description": "Words to search the catalog for." },
                "status": { "type": "string", "enum": CATALOG_STATUSES, "description": "With list, only extensions that are turned on or off." },
                "confirm": confirm_property(),
            },
            "required": ["action"],
            "additionalProperties": false,
        },
        "annotations": {
            "title": "Veronica extensions",
            "readOnlyHint": false,
            "destructiveHint": true,
            "idempotentHint": false,
        },
    })
}

pub(crate) enum CatalogCall {
    Run {
        invocation: Invocation,
        status: Option<bool>,
    },
    Preview(Value),
}

fn text_field<'a>(input: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, RpcError> {
    match input.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(RpcError::new(
            INVALID_PARAMS,
            format!("{key} must be a string"),
        )),
    }
}

fn required<'a>(
    input: &'a Map<String, Value>,
    key: &str,
    action: &str,
) -> Result<&'a str, RpcError> {
    text_field(input, key)?
        .ok_or_else(|| RpcError::new(INVALID_PARAMS, format!("{action} needs {key}")))
}

pub(crate) fn catalog_call(input: &Map<String, Value>) -> Result<CatalogCall, RpcError> {
    for key in input.keys() {
        if !["action", "id", "query", "status", "confirm"].contains(&key.as_str()) {
            return Err(RpcError::new(
                INVALID_PARAMS,
                format!("{CATALOG_TOOL} does not take {key:?}"),
            ));
        }
    }
    let action = required(input, "action", CATALOG_TOOL)?;
    let confirm = confirm_flag(input)?;
    let status = text_field(input, "status")?
        .map(|status| match status {
            "enabled" => Ok(true),
            "disabled" => Ok(false),
            other => Err(RpcError::new(
                INVALID_PARAMS,
                format!("status {other:?} is not enabled or disabled"),
            )),
        })
        .transpose()?;
    let words: Vec<&str> = match action {
        "list" => vec!["ls"],
        "search" => vec!["search", required(input, "query", action)?],
        "info" | "install" | "enable" | "disable" | "remove" => {
            vec![action, required(input, "id", action)?]
        }
        "update" => std::iter::once("update")
            .chain(text_field(input, "id")?)
            .collect(),
        other => {
            return Err(RpcError::new(
                INVALID_PARAMS,
                format!(
                    "unknown action {other:?}; expected one of {}",
                    CATALOG_ACTIONS.join(", ")
                ),
            ))
        }
    };
    let invocation = Invocation {
        argv: std::iter::once("extensions")
            .chain(words)
            .map(str::to_string)
            .collect(),
        confirm,
    };
    if matches!(action, "enable" | "disable") && !confirm {
        return Ok(CatalogCall::Preview(json!({
            "confirmationRequired": true,
            "preview": { "command": invocation.argv.join(" "), "effect": "write" },
            "rerun": invocation.rerun(),
        })));
    }
    Ok(CatalogCall::Run {
        invocation,
        status: status.filter(|_| action == "list"),
    })
}

pub(crate) fn filter_listing(completed: &mut Completed, enabled: bool) {
    let Ok(Value::Array(rows)) = serde_json::from_str::<Value>(&completed.stdout) else {
        return;
    };
    let kept: Vec<Value> = rows
        .into_iter()
        .filter(|row| row["enabled"] == enabled)
        .collect();
    completed.stdout = Value::Array(kept).to_string();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            other => panic!("{other} is not an object"),
        }
    }

    fn tool(effect: Effect) -> Tool {
        Tool {
            name: "veronica_fixture_wipe".to_string(),
            path: vec!["fixture".to_string(), "wipe".to_string()],
            about: Some("Pretend to delete one item".to_string()),
            effect,
            args: Vec::new(),
        }
    }

    #[test]
    fn host_flags_inside_arguments_are_refused() {
        for word in ["--yes", "-y", "--json", "-vy", "--yes=true"] {
            let error = tool(Effect::Read)
                .argv(&input(json!({ "arguments": ["a", word] })))
                .unwrap_err();
            assert_eq!(error.code, INVALID_PARAMS, "{word}");
        }
        let literal = tool(Effect::Read)
            .argv(&input(json!({ "arguments": ["--", "--yes"] })))
            .unwrap();
        assert_eq!(literal.argv, ["fixture", "wipe", "--", "--yes"]);
        assert!(!refused("-v"));
        assert!(!refused("--year"));
    }

    #[test]
    fn confirm_exists_only_on_destructive_tools() {
        assert!(tool(Effect::Read)
            .argv(&input(json!({ "confirm": true })))
            .is_err());
        let call = tool(Effect::Destructive)
            .argv(&input(json!({ "confirm": true, "arguments": ["x"] })))
            .unwrap();
        assert!(call.confirm);
        assert!(tool(Effect::Read).describe()["inputSchema"]["properties"]["confirm"].is_null());
        assert_eq!(
            tool(Effect::Destructive).describe()["annotations"]["destructiveHint"],
            true
        );
    }

    #[test]
    fn exit_codes_decide_the_error_flag() {
        let preview = Completed {
            code: Some(5),
            stdout: r#"{"confirmationRequired":true}"#.to_string(),
            stderr: String::new(),
        };
        assert_eq!(result(&preview, "2025-11-25")["isError"], false);
        assert_eq!(
            result(&preview, "2025-11-25")["structuredContent"]["confirmationRequired"],
            true
        );
        assert!(result(&preview, "2025-03-26")["structuredContent"].is_null());
        let failed = Completed {
            code: Some(3),
            stdout: r#"{"error":{"code":3}}"#.to_string(),
            stderr: "vr: not found\n".to_string(),
        };
        let reply = result(&failed, "2025-11-25");
        assert_eq!(reply["isError"], true);
        assert_eq!(reply["content"][0]["text"], "vr: not found");
    }

    #[test]
    fn catalog_actions_map_to_extension_commands() {
        let Ok(CatalogCall::Run { invocation, .. }) =
            catalog_call(&input(json!({ "action": "install", "id": "fixture" })))
        else {
            panic!("install runs and previews itself");
        };
        assert_eq!(invocation.argv, ["extensions", "install", "fixture"]);
        assert!(matches!(
            catalog_call(&input(json!({ "action": "enable", "id": "fixture" }))),
            Ok(CatalogCall::Preview(_))
        ));
        assert!(catalog_call(&input(json!({ "action": "search" }))).is_err());
        assert!(catalog_call(&input(json!({ "action": "explode" }))).is_err());
    }
}
