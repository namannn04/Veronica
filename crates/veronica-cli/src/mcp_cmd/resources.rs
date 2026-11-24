use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};
use veronica_core::{AppDirectories, Settings};
use veronica_host::registry;
use veronica_host::Layout;
use veronica_protocol::canonical;

use super::protocol::{RpcError, INTERNAL_ERROR, INVALID_PARAMS, RESOURCE_NOT_FOUND};
use super::tools::{self, Invocation};
use crate::guide_cmd;

const GUIDE: &str = "veronica://guide";
const GUIDE_TOPIC: &str = "veronica://guide/";
const EXTENSION: &str = "veronica://extensions/";
const JSON: &str = "application/json";
const TEXT: &str = "text/plain";

pub(crate) fn installed_ids() -> Result<Vec<String>> {
    let directories = AppDirectories::current()?;
    let layout = Layout::new(&directories);
    let settings = Settings::load(&directories.settings_file())?;
    let registry = registry::view(&layout.installed_file(), &settings)?;
    Ok(registry.extensions.into_keys().collect())
}

pub(crate) fn list() -> Value {
    let mut resources = vec![json!({
        "uri": GUIDE,
        "name": "guide",
        "title": "Every vr command",
        "description": "The command catalog as data: paths, arguments, effects and MCP tool names.",
        "mimeType": JSON,
    })];
    resources.extend(guide_cmd::topics().map(|(id, title)| {
        json!({
            "uri": format!("{GUIDE_TOPIC}{id}"),
            "name": format!("guide/{id}"),
            "title": title,
            "mimeType": TEXT,
        })
    }));
    match installed_ids() {
        Ok(ids) => resources.extend(ids.into_iter().map(|id| {
            json!({
                "uri": format!("{EXTENSION}{id}"),
                "name": format!("extensions/{id}"),
                "title": format!("The {id} extension"),
                "mimeType": JSON,
            })
        })),
        Err(error) => eprintln!("vr: installed extensions are unavailable: {error:#}"),
    }
    json!({ "resources": resources })
}

pub(crate) fn templates() -> Value {
    json!({
        "resourceTemplates": [
            {
                "uriTemplate": "veronica://guide/{topic}",
                "name": "guide-topic",
                "title": "One part of the manual",
                "mimeType": TEXT,
            },
            {
                "uriTemplate": "veronica://extensions/{id}",
                "name": "extension",
                "title": "What an extension does, needs and adds",
                "mimeType": JSON,
            },
        ]
    })
}

fn not_found(uri: &str) -> RpcError {
    RpcError::new(RESOURCE_NOT_FOUND, format!("no resource at {uri}"))
        .with_data(json!({ "uri": uri }))
}

fn contents(uri: &str, mime: &str, text: &str) -> Value {
    json!({ "contents": [{ "uri": uri, "mimeType": mime, "text": text }] })
}

fn plausible_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        && !id.starts_with(['.', '-'])
}

pub(crate) async fn read(params: &Value, program: &Path) -> Result<Value, RpcError> {
    let uri = params["uri"]
        .as_str()
        .ok_or_else(|| RpcError::new(INVALID_PARAMS, "resources/read needs a uri"))?;
    if uri == GUIDE {
        let document = guide_cmd::document(&tools::current_routes());
        return Ok(contents(
            uri,
            JSON,
            &canonical::value_to_string_pretty(&document),
        ));
    }
    if let Some(topic) = uri.strip_prefix(GUIDE_TOPIC) {
        let document = guide_cmd::topic(topic).ok_or_else(|| not_found(uri))?;
        return Ok(contents(
            uri,
            TEXT,
            document["text"].as_str().unwrap_or_default(),
        ));
    }
    let id = uri
        .strip_prefix(EXTENSION)
        .filter(|id| plausible_id(id))
        .ok_or_else(|| not_found(uri))?;
    let invocation = Invocation {
        argv: vec!["extensions".to_string(), "info".to_string(), id.to_string()],
        confirm: false,
    };
    let completed = invocation
        .run(program)
        .await
        .map_err(|error| RpcError::new(INTERNAL_ERROR, format!("cannot run vr: {error}")))?;
    match completed.code {
        Some(0) => Ok(contents(uri, JSON, &completed.stdout)),
        Some(3) => Err(not_found(uri)),
        _ => Err(RpcError::new(
            INTERNAL_ERROR,
            completed.stderr.trim_end().to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_ids_reach_the_command_line() {
        assert!(plausible_id("fixture"));
        assert!(plausible_id("system-tools"));
        assert!(!plausible_id(""));
        assert!(!plausible_id("../etc"));
        assert!(!plausible_id("a b"));
        assert!(!plausible_id("--yes"));
    }

    #[test]
    fn the_listing_starts_with_the_guide_and_every_topic() {
        let listed = list();
        let uris: Vec<&str> = listed["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|resource| resource["uri"].as_str().unwrap())
            .collect();
        assert_eq!(uris[0], GUIDE);
        assert!(uris.contains(&"veronica://guide/agents"));
    }
}
