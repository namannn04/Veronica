//! `vr herdr` — the live agent board, from the terminal.
//!
//! Edith's `ed herdr` reads Herdr's own JSON API and groups the agents into
//! Blocked, Working, Unknown, Done and Idle. This is the same read, in the same
//! lanes: Herdr remains the owner of terminal state, so nothing here starts,
//! stops or attaches to anything.

use anyhow::Result;

use crate::format::{self, Output};

/// Edith's lane order, which is also the order of urgency.
const LANES: [&str; 5] = ["blocked", "working", "unknown", "done", "idle"];

#[derive(clap::Subcommand)]
pub enum HerdrCommand {
    /// The board: every agent, in Edith's lanes.
    #[command(alias = "ls")]
    Board {
        /// Only agents in this session.
        #[arg(long)]
        session: Option<String>,
        /// Only agents of this kind, e.g. `claude`.
        #[arg(long)]
        kind: Option<String>,
        /// Only agents in this lane.
        #[arg(long)]
        status: Option<String>,
    },
    /// The persistent sessions Herdr knows about.
    Sessions,
    /// Print the command that attaches to a session or agent.
    ///
    /// Printed rather than run, so it works over SSH and can be piped into
    /// whatever terminal you actually want it in.
    Attach {
        session: String,
        /// A pane id, to attach to one agent rather than the session.
        pane_id: Option<String>,
    },
}

pub async fn run(command: &HerdrCommand, output: Output) -> Result<()> {
    let board = veronica_system::herdr::board().await?;
    if !board.installed {
        anyhow::bail!(
            "Herdr is not installed, or is not on PATH, ~/.local/bin or ~/.cargo/bin. \
             Nothing else on this command needs it."
        );
    }
    // Herdr answered with something unusable rather than failing outright.
    // The read still stands, so this is a warning on stderr and not an error:
    // an empty board with no explanation is the worse outcome.
    if let Some(reason) = &board.error {
        tracing::warn!("herdr: {reason}");
    }

    match command {
        HerdrCommand::Board {
            session,
            kind,
            status,
        } => {
            if let Some(lane) = status {
                if !LANES.contains(&lane.to_lowercase().as_str()) {
                    anyhow::bail!("unknown status '{lane}'; try one of {}", LANES.join(", "));
                }
            }
            let agents: Vec<_> = board
                .agents
                .iter()
                .filter(|agent| {
                    session.as_ref().is_none_or(|name| &agent.session == name)
                        && kind
                            .as_ref()
                            .is_none_or(|value| agent.kind.eq_ignore_ascii_case(value))
                        && status
                            .as_ref()
                            .is_none_or(|value| agent.status.eq_ignore_ascii_case(value))
                })
                .collect();

            output.emit(&agents, || {
                if agents.is_empty() {
                    return "no agents are running".to_string();
                }
                // Grouped by lane, in Edith's order, so the board reads the same
                // way it does in the app.
                let mut rows: Vec<Vec<String>> = Vec::new();
                for lane in LANES {
                    for agent in agents.iter().filter(|agent| agent.status == lane) {
                        rows.push(vec![
                            lane.to_string(),
                            agent.kind.clone(),
                            agent.session.clone(),
                            agent.title.clone(),
                            agent.workspace.clone(),
                        ]);
                    }
                }
                // A lane Herdr reports that Veronica does not know about would
                // otherwise vanish from the table entirely.
                for agent in agents
                    .iter()
                    .filter(|agent| !LANES.contains(&agent.status.as_str()))
                {
                    rows.push(vec![
                        agent.status.clone(),
                        agent.kind.clone(),
                        agent.session.clone(),
                        agent.title.clone(),
                        agent.workspace.clone(),
                    ]);
                }
                format::table(&["status", "kind", "session", "title", "workspace"], &rows)
            })
        }

        HerdrCommand::Sessions => output.emit(&board.sessions, || {
            let rows: Vec<Vec<String>> = board
                .sessions
                .iter()
                .map(|session| {
                    vec![
                        session.name.clone(),
                        if session.running {
                            "running"
                        } else {
                            "stopped"
                        }
                        .to_string(),
                        if session.default { "default" } else { "" }.to_string(),
                        session
                            .error
                            .clone()
                            .unwrap_or_else(|| session.session_dir.clone()),
                    ]
                })
                .collect();
            format::table(&["session", "state", "", "directory"], &rows)
        }),

        HerdrCommand::Attach { session, pane_id } => {
            // Verified against the board, so a typo says so rather than
            // producing a command that fails later in someone's terminal.
            if !board.sessions.iter().any(|entry| &entry.name == session) {
                anyhow::bail!(
                    "no Herdr session named '{session}'; run `vr herdr sessions` for the list"
                );
            }
            if let Some(pane) = pane_id {
                if !board
                    .agents
                    .iter()
                    .any(|agent| &agent.session == session && &agent.pane_id == pane)
                {
                    anyhow::bail!(
                        "no pane '{pane}' in Herdr session '{session}'; \
                         run `vr herdr board` for the live panes"
                    );
                }
            }
            // The line comes from the same helper the app's detail pane and
            // the board's own rows use, so what is printed here is what
            // Veronica would have run.
            let command = veronica_system::herdr::attach_command(session, pane_id.as_deref());
            output.emit(&serde_json::json!({ "command": command }), || {
                command.clone()
            })
        }
    }
}
