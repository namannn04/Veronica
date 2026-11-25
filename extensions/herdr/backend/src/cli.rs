use anyhow::Result;
use veronica_sdk::{format, Output};

const LANES: [&str; 5] = ["blocked", "working", "unknown", "done", "idle"];

#[derive(clap::Subcommand)]
pub(crate) enum HerdrCommand {
    #[command(
        alias = "ls",
        about = "The board: every agent, grouped into lanes by state"
    )]
    Board {
        #[arg(long, help = "Only agents in this session")]
        session: Option<String>,
        #[arg(long, help = "Only agents of this kind, e.g. `claude`")]
        kind: Option<String>,
        #[arg(long, help = "Only agents in this lane")]
        status: Option<String>,
    },
    #[command(about = "The persistent sessions Herdr knows about")]
    Sessions,
    #[command(
        about = "Print the command that attaches to a session or agent",
        long_about = "Print the command that attaches to a session or agent.\n\nPrinted rather than run, so it works over SSH and can be piped into whatever terminal you actually want it in."
    )]
    Attach {
        session: String,
        #[arg(help = "A pane id, to attach to one agent rather than the session")]
        pane_id: Option<String>,
    },
}

pub(crate) async fn run(command: &HerdrCommand, output: Output<'_>) -> Result<()> {
    let board = veronica_system::herdr::board().await?;
    if !board.installed {
        anyhow::bail!(
            "Herdr is not installed, or is not on PATH, ~/.local/bin or ~/.cargo/bin. \
             Nothing else on this command needs it."
        );
    }
    if let Some(reason) = &board.error {
        eprintln!("herdr: {reason}");
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
            let command = veronica_system::herdr::attach_command(session, pane_id.as_deref());
            output.emit(&serde_json::json!({ "command": command }), || {
                command.clone()
            })
        }
    }
}
