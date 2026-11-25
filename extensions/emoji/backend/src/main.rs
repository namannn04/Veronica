mod cli;
mod preferences;
mod rpc;

use clap::{Parser, Subcommand};
use serde_json::Value;
use veronica_sdk::protocol::rpc::{CliRunParams, CliRunResult, ErrorObject, RpcCallParams};
use veronica_sdk::{Capture, Context, Extension};

#[derive(Parser)]
#[command(name = "vr", disable_help_subcommand = true)]
pub(crate) struct Program {
    #[arg(
        long,
        global = true,
        help = "Emit JSON on stdout instead of human-readable text"
    )]
    json: bool,
    #[command(subcommand)]
    root: Root,
}

#[derive(Subcommand)]
enum Root {
    #[command(
        subcommand,
        about = "Search the emoji catalogue and copy what you pick"
    )]
    Emoji(cli::EmojiCommand),
}

struct EmojiExtension;

impl Extension for EmojiExtension {
    fn methods(&self) -> Vec<String> {
        rpc::METHODS.map(String::from).to_vec()
    }

    async fn cli_run(
        &self,
        _context: Context,
        params: CliRunParams,
    ) -> Result<CliRunResult, ErrorObject> {
        let program = match veronica_sdk::cli::parse::<Program>(&params) {
            Ok(program) => program,
            Err(result) => return Ok(result),
        };
        let capture = Capture::new(params.json);
        let Root::Emoji(command) = &program.root;
        let outcome = cli::run(command, capture.output()).await;
        Ok(capture.finish(outcome))
    }

    async fn rpc_call(
        &self,
        _context: Context,
        params: RpcCallParams,
    ) -> Result<Value, ErrorObject> {
        rpc::call(&params.method, params.params).await
    }
}

fn main() -> std::process::ExitCode {
    veronica_sdk::run(EmojiExtension)
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use veronica_sdk::protocol::manifest::Effect;

    use super::Program;

    const EFFECTS: [(&str, Effect); 5] = [
        ("emoji copy", Effect::Write),
        ("emoji forget", Effect::Destructive),
        ("emoji groups", Effect::Read),
        ("emoji list", Effect::Read),
        ("emoji recents", Effect::Read),
    ];

    #[test]
    fn the_manifest_lists_every_command() {
        let commands = veronica_sdk::cli::describe(Program::command(), &EFFECTS).unwrap();
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../manifest.json");
        veronica_sdk::cli::sync_manifest(&manifest, commands).unwrap();
    }
}
