mod clients;
mod database;
mod protocol;
mod resources;
mod server;
mod tools;

use anyhow::Result;

use crate::format::Output;
use clients::{Change, ClientArg};

#[derive(clap::Subcommand)]
pub(crate) enum McpCommand {
    #[command(
        about = "Serve Veronica's commands as MCP tools over stdin and stdout (the default)",
        long_about = "Serve Veronica's commands as MCP tools over stdin and stdout.\n\nStays in the foreground and prints no banner: stdout carries protocol traffic and nothing else."
    )]
    Serve,
    #[command(about = "Register the MCP server with an agent client; previews unless --yes")]
    Install {
        #[arg(long, value_enum)]
        client: ClientArg,
        #[arg(long, help = "Show what would change and change nothing")]
        dry_run: bool,
    },
    #[command(about = "Whether each agent client has the MCP server registered")]
    Status {
        #[arg(long, value_enum, default_value = "all")]
        client: ClientArg,
    },
    #[command(about = "Remove the MCP server from an agent client; previews unless --yes")]
    Uninstall {
        #[arg(long, value_enum)]
        client: ClientArg,
        #[arg(long, help = "Show what would change and change nothing")]
        dry_run: bool,
    },
}

pub(crate) async fn serve() -> Result<()> {
    server::serve().await
}

pub(crate) async fn run(command: &McpCommand, output: Output, yes: bool) -> Result<()> {
    match command {
        McpCommand::Serve => serve().await,
        McpCommand::Install { client, dry_run } => {
            clients::change(Change::Install, *client, *dry_run, output, yes)
        }
        McpCommand::Status { client } => clients::status(*client, output),
        McpCommand::Uninstall { client, dry_run } => {
            clients::change(Change::Uninstall, *client, *dry_run, output, yes)
        }
    }
}
