mod chat;
mod cli;
mod recorder;
mod rpc;

use clap::{Parser, Subcommand};
use serde_json::Value;
use veronica_core::AppDirectories;
use veronica_sdk::protocol::rpc::{
    CliRunParams, CliRunResult, ErrorObject, RpcCallParams, RunMode,
};
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
    #[command(subcommand, about = "Your private notes and voice-memo metadata")]
    Companion(cli::CompanionCommand),
}

struct Companion;

impl Extension for Companion {
    fn methods(&self) -> Vec<String> {
        rpc::METHODS
            .iter()
            .chain(chat::METHODS.iter())
            .map(|method| (*method).to_string())
            .collect()
    }

    async fn initialize(&self, context: &Context) -> Result<(), ErrorObject> {
        if context.mode() == RunMode::Service {
            tokio::spawn(chat::close_when_unseen());
        }
        Ok(())
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
        let Root::Companion(command) = &program.root;
        let outcome = AppDirectories::current().and_then(|directories| {
            cli::run(
                &directories,
                command,
                params.stdin.as_deref(),
                capture.output(),
            )
        });
        Ok(capture.finish(outcome))
    }

    async fn rpc_call(
        &self,
        _context: Context,
        params: RpcCallParams,
    ) -> Result<Value, ErrorObject> {
        rpc::call(&params.method, params.params).await
    }

    async fn shutdown(&self) {
        if let Ok(repository) = rpc::repository() {
            let _ = recorder::stop(&repository);
        }
        if let Ok(chat) = chat::runtime() {
            chat.close_window();
        }
    }
}

fn main() -> std::process::ExitCode {
    veronica_sdk::run(Companion)
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use veronica_sdk::protocol::manifest::Effect;

    use super::Program;

    const EFFECTS: [(&str, Effect); 5] = [
        ("companion list", Effect::Read),
        ("companion show", Effect::Read),
        ("companion add", Effect::Write),
        ("companion pin", Effect::Write),
        ("companion remove", Effect::Destructive),
    ];

    #[test]
    fn the_manifest_lists_every_command() {
        let commands = veronica_sdk::cli::describe(Program::command(), &EFFECTS).unwrap();
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../manifest.json");
        veronica_sdk::cli::sync_manifest(&manifest, commands).unwrap();
    }
}
