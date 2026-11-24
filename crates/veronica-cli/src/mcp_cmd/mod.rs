mod protocol;
mod server;
mod tools;

use anyhow::Result;

#[derive(clap::Subcommand)]
pub(crate) enum McpCommand {
    #[command(
        about = "Serve Veronica's commands as MCP tools over stdin and stdout (the default)",
        long_about = "Serve Veronica's commands as MCP tools over stdin and stdout.\n\nStays in the foreground and prints no banner: stdout carries protocol traffic and nothing else."
    )]
    Serve,
}

pub(crate) async fn run(command: &McpCommand) -> Result<()> {
    match command {
        McpCommand::Serve => server::serve().await,
    }
}
