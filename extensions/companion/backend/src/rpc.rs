use anyhow::Context as _;
use serde::Deserialize;
use serde_json::Value;
use veronica_core::{AppDirectories, AttentionRepository, CompanionRepository};
use veronica_sdk::protocol::rpc::ErrorObject;
use veronica_sdk::rpc::{failure, reply};

use crate::{chat, recorder};

pub(crate) const METHODS: [&str; 10] = [
    "list",
    "createNote",
    "update",
    "remove",
    "recordingStatus",
    "recordStart",
    "recordStop",
    "recordCancel",
    "audioPath",
    "activity",
];

#[derive(Deserialize)]
struct Query {
    #[serde(default)]
    query: String,
}

#[derive(Deserialize)]
struct Note {
    title: String,
    body: String,
}

#[derive(Deserialize)]
struct Update {
    id: u64,
    title: String,
    body: String,
    pinned: bool,
}

#[derive(Deserialize)]
struct Days {
    days: i64,
}

#[derive(Deserialize)]
struct Item {
    id: u64,
}

pub(crate) fn repository() -> anyhow::Result<CompanionRepository> {
    Ok(CompanionRepository::new(
        AppDirectories::current()?.companion_dir(),
    ))
}

fn activity(days: i64) -> anyhow::Result<Value> {
    let now = chrono::Utc::now();
    let repository = AttentionRepository::new(AppDirectories::current()?.attention_dir());
    Ok(serde_json::to_value(repository.overview(
        AttentionRepository::window_start(days, now),
        now,
    )?)?)
}

fn dispatch(method: &str, arguments: Value) -> anyhow::Result<Value> {
    let repository = repository()?;
    let now = chrono::Utc::now();
    Ok(match method {
        "list" => {
            let Query { query } = serde_json::from_value(arguments)?;
            serde_json::to_value(repository.list(&query)?)?
        }
        "createNote" => {
            let Note { title, body } = serde_json::from_value(arguments)?;
            serde_json::to_value(repository.create_note(&title, &body, now)?)?
        }
        "update" => {
            let request: Update = serde_json::from_value(arguments)?;
            serde_json::to_value(repository.update(
                request.id,
                &request.title,
                &request.body,
                request.pinned,
                now,
            )?)?
        }
        "remove" => {
            let Item { id } = serde_json::from_value(arguments)?;
            repository.remove(id)?;
            Value::Null
        }
        "recordingStatus" => serde_json::to_value(recorder::status())?,
        "recordStart" => {
            recorder::start(&repository)?;
            Value::Null
        }
        "recordStop" => serde_json::to_value(
            recorder::stop(&repository)?.context("no voice memo is recording")?,
        )?,
        "recordCancel" => {
            recorder::cancel();
            Value::Null
        }
        "audioPath" => {
            let Item { id } = serde_json::from_value(arguments)?;
            serde_json::to_value(repository.audio_path(id)?.display().to_string())?
        }
        "activity" => {
            let Days { days } = serde_json::from_value(arguments)?;
            activity(days)?
        }
        other => anyhow::bail!("unknown method {other}"),
    })
}

pub(crate) async fn call(method: &str, arguments: Value) -> Result<Value, ErrorObject> {
    let value = if METHODS.contains(&method) {
        dispatch(method, arguments)
    } else if chat::METHODS.contains(&method) {
        chat::dispatch(method, arguments).await
    } else {
        return Err(ErrorObject::method_not_found(method));
    };
    reply(&value.map_err(|error| failure(&error))?)
}
