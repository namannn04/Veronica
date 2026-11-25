use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::Value;
use veronica_core::AppDirectories;
use veronica_system::companion_ai::{ChatRuntime, ModelConfig};

const PRESENCE_TIMEOUT: Duration = Duration::from_mins(1);
const PRESENCE_CHECK: Duration = Duration::from_secs(15);

static LAST_SEEN: Mutex<Option<Instant>> = Mutex::new(None);

pub(crate) const METHODS: [&str; 11] = [
    "chatPresence",
    "chatView",
    "chatConfigure",
    "chatEnable",
    "chatSend",
    "chatPreview",
    "chatModels",
    "chatPull",
    "chatClear",
    "chatExport",
    "chatJournal",
];

pub(crate) fn runtime() -> anyhow::Result<&'static ChatRuntime> {
    static RUNTIME: OnceLock<ChatRuntime> = OnceLock::new();
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime);
    }
    let root = AppDirectories::current()?.data.join("companion/chat");
    Ok(RUNTIME.get_or_init(|| ChatRuntime::new(root)))
}

fn seen() {
    *LAST_SEEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Instant::now());
}

pub(crate) async fn close_when_unseen() {
    loop {
        tokio::time::sleep(PRESENCE_CHECK).await;
        let expired = LAST_SEEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take_if(|last| last.elapsed() > PRESENCE_TIMEOUT)
            .is_some();
        if expired {
            if let Ok(runtime) = runtime() {
                runtime.close_window();
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Configure {
    config: ModelConfig,
    memory: String,
}

#[derive(Deserialize)]
struct Enable {
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Send {
    text: String,
    #[serde(default)]
    key: String,
    include_history: bool,
    include_memory: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Preview {
    text: String,
    include_history: bool,
    include_memory: bool,
}

#[derive(Deserialize)]
struct Models {
    config: ModelConfig,
}

#[derive(Deserialize)]
struct Pull {
    config: ModelConfig,
    confirmed: bool,
}

#[derive(Deserialize)]
struct Clear {
    memory: bool,
}

#[derive(Deserialize)]
struct Journal {
    text: String,
}

pub(crate) async fn dispatch(method: &str, arguments: Value) -> anyhow::Result<Value> {
    let chat = runtime()?;
    Ok(match method {
        "chatPresence" => {
            if LAST_SEEN
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_none()
            {
                chat.open_window();
            }
            seen();
            Value::Null
        }
        "chatView" => serde_json::to_value(chat.view()?)?,
        "chatConfigure" => {
            let Configure { config, memory } = serde_json::from_value(arguments)?;
            chat.configure(config, memory)?;
            Value::Null
        }
        "chatEnable" => {
            let Enable { enabled } = serde_json::from_value(arguments)?;
            chat.enable(enabled);
            serde_json::to_value(chat.view()?)?
        }
        "chatSend" => {
            let request: Send = serde_json::from_value(arguments)?;
            serde_json::to_value(
                chat.send(
                    request.text,
                    request.key,
                    request.include_history,
                    request.include_memory,
                )
                .await?,
            )?
        }
        "chatPreview" => {
            let request: Preview = serde_json::from_value(arguments)?;
            serde_json::to_value(chat.preview(
                &request.text,
                request.include_history,
                request.include_memory,
            )?)?
        }
        "chatModels" => {
            let Models { config } = serde_json::from_value(arguments)?;
            serde_json::to_value(chat.models(config).await?)?
        }
        "chatPull" => {
            let Pull { config, confirmed } = serde_json::from_value(arguments)?;
            chat.pull(config, confirmed).await?;
            Value::Null
        }
        "chatClear" => {
            let Clear { memory } = serde_json::from_value(arguments)?;
            chat.clear(memory)?;
            Value::Null
        }
        "chatExport" => serde_json::to_value(chat.export()?)?,
        "chatJournal" => {
            let Journal { text } = serde_json::from_value(arguments)?;
            serde_json::to_value(chat.journal(text)?)?
        }
        other => anyhow::bail!("unknown method {other}"),
    })
}
