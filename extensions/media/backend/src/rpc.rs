use serde::Deserialize;
use serde_json::Value;
use veronica_media::Transport;
use veronica_sdk::protocol::rpc::{error_code, ErrorObject};
use veronica_sdk::rpc::{failure, params, reply};

pub(crate) const METHODS: [&str; 3] = ["nowPlaying", "control", "library"];

#[derive(Deserialize)]
struct ControlParams {
    action: String,
}

async fn session() -> Result<zbus::Connection, ErrorObject> {
    zbus::Connection::session().await.map_err(|error| {
        ErrorObject::new(
            error_code::UNAVAILABLE,
            format!("cannot reach the session bus: {error}"),
        )
    })
}

pub(crate) async fn now_playing(
    connection: &zbus::Connection,
) -> anyhow::Result<Option<veronica_media::NowPlaying>> {
    let mut playing = veronica_media::now_playing(connection).await?;
    if let Some(playing) = playing.as_mut() {
        playing.art_url = playing.art_url.as_deref().and_then(crate::art::to_data_url);
    }
    Ok(playing)
}

fn transport(action: &str) -> Result<Transport, ErrorObject> {
    Ok(match action {
        "play" => Transport::Play,
        "pause" => Transport::Pause,
        "toggle" => Transport::PlayPause,
        "next" => Transport::Next,
        "previous" => Transport::Previous,
        "stop" => Transport::Stop,
        other => {
            return Err(ErrorObject::new(
                error_code::INVALID_PARAMS,
                format!("unknown media action {other:?}"),
            ))
        }
    })
}

pub(crate) async fn call(method: &str, arguments: Value) -> Result<Value, ErrorObject> {
    match method {
        "nowPlaying" => {
            let connection = session().await?;
            reply(
                &now_playing(&connection)
                    .await
                    .map_err(|error| failure(&error))?,
            )
        }
        "control" => {
            let ControlParams { action } = params(arguments)?;
            let transport = transport(&action)?;
            let connection = session().await?;
            veronica_media::control(&connection, transport)
                .await
                .map_err(|error| failure(&error))?;
            Ok(Value::Null)
        }
        "library" => {
            let root = crate::cli::music_root().map_err(|error| failure(&error))?;
            if !root.exists() {
                return reply(&Vec::<veronica_media::LocalTrack>::new());
            }
            reply(&veronica_media::scan_library(&root).map_err(|error| failure(&error))?)
        }
        other => Err(ErrorObject::method_not_found(other)),
    }
}
