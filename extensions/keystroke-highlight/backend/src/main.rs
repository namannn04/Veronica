mod cli;
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
        name = "keystroke-highlight",
        alias = "keys",
        about = "Keystroke Highlight: show each key press on screen for demos"
    )]
    Keystroke(cli::KeystrokeCommand),
}

struct KeystrokeHighlight;

impl Extension for KeystrokeHighlight {
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
        let Root::Keystroke(command) = &program.root;
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
    veronica_sdk::run(KeystrokeHighlight)
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use veronica_sdk::protocol::manifest::Effect;

    use super::Program;

    const EFFECTS: [(&str, Effect); 7] = [
        ("keystroke-highlight disable", Effect::Write),
        ("keystroke-highlight duration", Effect::Write),
        ("keystroke-highlight enable", Effect::Write),
        ("keystroke-highlight off", Effect::Write),
        ("keystroke-highlight on", Effect::Write),
        ("keystroke-highlight position", Effect::Write),
        ("keystroke-highlight status", Effect::Read),
    ];

    #[test]
    fn the_manifest_lists_every_command() {
        let commands = veronica_sdk::cli::describe(Program::command(), &EFFECTS).unwrap();
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../manifest.json");
        veronica_sdk::cli::sync_manifest(&manifest, commands).unwrap();
    }
}
