use serde::Deserialize;
use serde_json::Value;
use veronica_sdk::protocol::rpc::ErrorObject;
use veronica_sdk::rpc::{failure, reply};
use veronica_system::{audio, bluetooth, docker, metrics, ports};

pub(crate) const METHODS: [&str; 13] = [
    "snapshot",
    "processes",
    "quitProcess",
    "audioStreams",
    "streamVolume",
    "streamToggleMute",
    "bluetooth",
    "microphone",
    "microphoneToggle",
    "ports",
    "portsStop",
    "docker",
    "dockerAction",
];

#[derive(Deserialize)]
struct Pid {
    pid: u32,
}

#[derive(Deserialize)]
struct Stream {
    id: u32,
    #[serde(default)]
    volume: f32,
}

#[derive(Deserialize)]
struct ContainerRequest {
    endpoint: String,
    id: String,
    action: String,
}

async fn dispatch(method: &str, arguments: Value) -> anyhow::Result<Value> {
    Ok(match method {
        "snapshot" => serde_json::to_value(crate::sampler::sample().await?)?,
        "processes" => {
            serde_json::to_value(tokio::task::spawn_blocking(metrics::running_processes).await?)?
        }
        "quitProcess" => {
            let Pid { pid } = serde_json::from_value(arguments)?;
            metrics::terminate_process(pid)?;
            Value::Null
        }
        "audioStreams" => serde_json::to_value(audio::streams().await?)?,
        "streamVolume" => {
            let Stream { id, volume } = serde_json::from_value(arguments)?;
            audio::set_stream_volume(id, volume).await?;
            Value::Null
        }
        "streamToggleMute" => {
            let Stream { id, .. } = serde_json::from_value(arguments)?;
            serde_json::to_value(audio::toggle_stream_muted(id).await?)?
        }
        "bluetooth" => serde_json::to_value(bluetooth::state().await)?,
        "microphone" => serde_json::to_value(audio::microphone().await?)?,
        "microphoneToggle" => serde_json::to_value(audio::toggle_microphone().await?)?,
        "ports" => serde_json::to_value(ports::snapshot().await?)?,
        "portsStop" => {
            let request: ports::StopRequest = serde_json::from_value(arguments)?;
            serde_json::to_value(ports::stop(&request).await?)?
        }
        "docker" => serde_json::to_value(docker::snapshot().await)?,
        "dockerAction" => {
            let request: ContainerRequest = serde_json::from_value(arguments)?;
            docker::action(&request.endpoint, &request.id, &request.action).await?;
            Value::Null
        }
        other => anyhow::bail!("unknown method {other}"),
    })
}

pub(crate) async fn call(method: &str, arguments: Value) -> Result<Value, ErrorObject> {
    if !METHODS.contains(&method) {
        return Err(ErrorObject::method_not_found(method));
    }
    let value = dispatch(method, arguments)
        .await
        .map_err(|error| failure(&error))?;
    reply(&value)
}
