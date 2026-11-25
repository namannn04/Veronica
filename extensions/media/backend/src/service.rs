use std::time::Duration;

use serde_json::{json, Value};
use veronica_media::NowPlaying;
use veronica_sdk::protocol::ContractId;
use veronica_sdk::Context;

pub(crate) const SURFACE: &str = "now-playing";
pub(crate) const CONTRACT: &str = "media.nowPlaying@1";
const POLL: Duration = Duration::from_secs(2);

pub(crate) fn surface_state(playing: Option<&NowPlaying>) -> Value {
    playing.map_or(Value::Null, |playing| {
        let title = if playing.title.is_empty() {
            playing.identity.as_str()
        } else {
            playing.title.as_str()
        };
        json!({
            "title": title,
            "artist": (!playing.artist.is_empty()).then_some(&playing.artist),
            "album": (!playing.album.is_empty()).then_some(&playing.album),
            "playing": playing.status.is_some_and(veronica_media::PlaybackStatus::is_playing),
        })
    })
}

pub(crate) async fn publish(context: Context) {
    let Ok(contract) = CONTRACT.parse::<ContractId>() else {
        return;
    };
    let connection = match zbus::Connection::session().await {
        Ok(connection) => connection,
        Err(error) => {
            tracing::warn!("now playing is unavailable: {error}");
            return;
        }
    };
    let mut last: Option<Value> = None;
    loop {
        let playing = veronica_media::now_playing(&connection)
            .await
            .ok()
            .flatten();
        let state = surface_state(playing.as_ref());
        if last.as_ref() != Some(&state) {
            let host = context.host();
            if let Err(error) = host.surface_update(SURFACE, state.clone()).await {
                tracing::debug!("surface update refused: {}", error.message);
            }
            let payload = serde_json::to_value(&playing).unwrap_or(Value::Null);
            if let Err(error) = host.contract_publish(contract.clone(), payload).await {
                tracing::debug!("contract publish refused: {}", error.message);
            }
            last = Some(state);
        }
        tokio::time::sleep(POLL).await;
    }
}

#[cfg(test)]
mod tests {
    use veronica_media::{NowPlaying, PlaybackStatus};

    use super::surface_state;

    fn track(title: &str, status: PlaybackStatus) -> NowPlaying {
        NowPlaying {
            player: "org.mpris.MediaPlayer2.test".into(),
            identity: "Test Player".into(),
            status: Some(status),
            title: title.into(),
            artist: "Artist".into(),
            album: String::new(),
            art_url: None,
            length_us: None,
            position_us: None,
            can_go_next: true,
            can_go_previous: true,
        }
    }

    #[test]
    fn the_surface_carries_the_track_and_whether_it_plays() {
        let state = surface_state(Some(&track("Song", PlaybackStatus::Playing)));
        assert_eq!(state["title"], "Song");
        assert_eq!(state["artist"], "Artist");
        assert!(state["album"].is_null());
        assert_eq!(state["playing"], true);
    }

    #[test]
    fn an_untitled_track_falls_back_to_the_player_and_nothing_clears_it() {
        let state = surface_state(Some(&track("", PlaybackStatus::Paused)));
        assert_eq!(state["title"], "Test Player");
        assert_eq!(state["playing"], false);
        assert!(surface_state(None).is_null());
    }
}
