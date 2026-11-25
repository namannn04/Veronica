mod cli;

use clap::{Parser, Subcommand};
use veronica_sdk::protocol::rpc::{CliRunParams, CliRunResult, ErrorObject};
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
    #[command(subcommand, about = "Local notch image conversion and QR generation")]
    Utility(cli::UtilityCommand),
}

struct Utility;

impl Extension for Utility {
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
        let Root::Utility(command) = &program.root;
        let outcome = cli::run(command, capture.output());
        Ok(capture.finish(outcome))
    }
}

fn main() -> std::process::ExitCode {
    veronica_sdk::run(Utility)
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use veronica_sdk::protocol::manifest::Effect;

    use super::Program;

    const EFFECTS: [(&str, Effect); 2] = [
        ("utility image", Effect::Write),
        ("utility qr", Effect::Write),
    ];

    #[test]
    fn the_manifest_lists_every_command() {
        let commands = veronica_sdk::cli::describe(Program::command(), &EFFECTS).unwrap();
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../manifest.json");
        veronica_sdk::cli::sync_manifest(&manifest, commands).unwrap();
    }
}
