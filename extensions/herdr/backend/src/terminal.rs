use std::sync::LazyLock;
use std::time::Duration;

use serde_json::json;
use veronica_sdk::HostClient;
use veronica_system::agents::{AgentManager, AgentProfile, AgentView};

pub(crate) const TOPIC: &str = "terminal";
const POLL: Duration = Duration::from_millis(60);
const BOARD_PATIENCE: Duration = Duration::from_secs(8);
const MAX_NAME: usize = 128;

static TERMINALS: LazyLock<AgentManager> = LazyLock::new(|| {
    let manager = AgentManager::default();
    manager.open();
    manager
});

pub(crate) fn terminals() -> &'static AgentManager {
    &TERMINALS
}

pub(crate) fn valid_target(session: &str, pane: &str) -> bool {
    !session.is_empty() && !pane.is_empty() && session.len() <= MAX_NAME && pane.len() <= MAX_NAME
}

pub(crate) async fn attach(
    host: HostClient,
    session: String,
    pane: String,
) -> anyhow::Result<AgentView> {
    anyhow::ensure!(valid_target(&session, &pane), "Invalid Herdr session/pane");
    let board = tokio::time::timeout(BOARD_PATIENCE, veronica_system::herdr::board())
        .await
        .map_err(|_| anyhow::anyhow!("Herdr did not respond within eight seconds"))??;
    let agent = board
        .agents
        .iter()
        .find(|agent| agent.session == session && agent.pane_id == pane)
        .ok_or_else(|| anyhow::anyhow!("This Herdr agent is no longer available"))?;
    let executable = veronica_system::herdr::executable()
        .ok_or_else(|| anyhow::anyhow!("Herdr is not installed"))?;
    let profile = AgentProfile {
        id: "herdr-attach".into(),
        name: "Herdr attached agent".into(),
        executable: executable.display().to_string(),
        arguments: veronica_system::herdr::attach_args(&session, Some(&pane)),
    };
    let cwd = agent.cwd.clone();
    let epoch = terminals().epoch();
    let view =
        tokio::task::spawn_blocking(move || terminals().start(profile, cwd, epoch)).await??;
    tokio::spawn(stream(host, view.id.clone()));
    Ok(view)
}

async fn stream(host: HostClient, id: String) {
    let mut cursor = 0;
    loop {
        let Ok(output) = terminals().output(&id, cursor) else {
            let _ = host.emit(TOPIC, json!({ "id": id, "status": "closed" }));
            return;
        };
        let running = output.status == "running";
        if output.cursor != cursor || !running {
            cursor = output.cursor;
            let _ = host.emit(
                TOPIC,
                json!({ "id": id, "bytes": output.bytes, "cursor": output.cursor, "status": output.status }),
            );
        }
        if !running {
            return;
        }
        tokio::time::sleep(POLL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::valid_target;

    #[test]
    fn targets_must_be_short_and_present() {
        assert!(valid_target("work", "pane-1"));
        assert!(!valid_target("", "pane-1"));
        assert!(!valid_target("work", ""));
        assert!(!valid_target(&"x".repeat(129), "pane-1"));
    }
}
