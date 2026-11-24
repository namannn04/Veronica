use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
struct Controls {
    agents: veronica_system::agents::AgentManager,
    chat: veronica_system::companion_ai::ChatRuntime,
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let controls = Arc::new(Controls {
        agents: Default::default(),
        chat: veronica_system::companion_ai::ChatRuntime::new(root.path().join("chat")),
    });
    controls.agents.open();
    controls.chat.open_window();
    let output = Arc::new(tokio::sync::Mutex::new(tokio::io::stdout()));
    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let request: Value = serde_json::from_str(&line)?;
        let state = controls.clone();
        let out = output.clone();
        tokio::spawn(async move {
            let id = request["id"].clone();
            let result = dispatch(&state, &request).await;
            let response = match result {
                Ok(value) => json!({"id":id,"result":value}),
                Err(error) => json!({"id":id,"error":error.to_string()}),
            };
            let mut out = out.lock().await;
            let _ = out.write_all(format!("{response}\n").as_bytes()).await;
            let _ = out.flush().await;
        });
    }
    controls.agents.stop_all();
    controls.chat.close_window();
    Ok(())
}
async fn dispatch(state: &Controls, r: &Value) -> anyhow::Result<Value> {
    let a = &r["args"];
    let text = |key: &str| a[key].as_str().unwrap_or_default().to_owned();
    let value = match r["command"].as_str().unwrap_or_default() {
        "agent_profiles" => {
            json!([{"id":"shell","name":"Test shell","executable":"/bin/sh","arguments":["-i"]}])
        }
        "agent_list" => serde_json::to_value(state.agents.list())?,
        "agent_external" => json!([]),
        "agent_start" => serde_json::to_value(state.agents.start(
            serde_json::from_value(a["profile"].clone())?,
            text("cwd"),
            state.agents.epoch(),
        )?)?,
        "agent_output" => serde_json::to_value(
            state
                .agents
                .output(&text("id"), a["cursor"].as_u64().unwrap_or(0))?,
        )?,
        "agent_input" => {
            state.agents.input(&text("id"), text("bytes"))?;
            Value::Null
        }
        "agent_resize" => {
            state.agents.resize(
                &text("id"),
                a["cols"].as_u64().unwrap_or(80) as u16,
                a["rows"].as_u64().unwrap_or(24) as u16,
            )?;
            Value::Null
        }
        "agent_stop" => {
            state.agents.stop(&text("id"))?;
            Value::Null
        }
        "chat_journal" => serde_json::to_value(state.chat.journal(text("text"))?)?,
        "chat_view" => serde_json::to_value(state.chat.view()?)?,
        "chat_configure" => {
            state
                .chat
                .configure(serde_json::from_value(a["config"].clone())?, text("memory"))?;
            Value::Null
        }
        "chat_enable" => {
            state.chat.enable(a["enabled"].as_bool().unwrap_or(false));
            serde_json::to_value(state.chat.view()?)?
        }
        "chat_send" => serde_json::to_value(
            state
                .chat
                .send(
                    text("text"),
                    text("key"),
                    a["includeHistory"].as_bool().unwrap_or(false),
                    a["includeMemory"].as_bool().unwrap_or(false),
                )
                .await?,
        )?,
        "chat_preview" => json!(state.chat.preview(
            &text("text"),
            a["includeHistory"].as_bool().unwrap_or(false),
            a["includeMemory"].as_bool().unwrap_or(false)
        )?),
        "chat_clear" => {
            state.chat.clear(a["memory"].as_bool().unwrap_or(false))?;
            Value::Null
        }
        "chat_export" => json!(state.chat.export()?),
        "close_window" => {
            state.agents.stop_all();
            state.chat.close_window();
            json!(state.agents.list())
        }
        _ => anyhow::bail!("Unknown integration-test command"),
    };
    Ok(value)
}
