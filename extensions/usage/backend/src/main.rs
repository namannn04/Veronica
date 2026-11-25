mod alerts_cli;
mod usage_cli;

use clap::{Parser, Subcommand};
use veronica_sdk::paths::AppDirectories;
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
    #[command(
        subcommand,
        about = "Agent usage: totals, limits, projects and collection"
    )]
    Usage(usage_cli::UsageCommand),
    #[command(
        subcommand,
        about = "Rate-limit alerts: the notifier's state, a delivery check and a reset"
    )]
    Alerts(alerts_cli::AlertsCommand),
}

async fn run(
    program: &Program,
    params: &CliRunParams,
    context: &Context,
    capture: &Capture,
) -> anyhow::Result<()> {
    let directories = AppDirectories::current()?;
    directories.prepare()?;
    match &program.root {
        Root::Usage(command) => {
            usage_cli::run(
                &directories,
                command,
                capture.output(),
                &params.cwd,
                context.host(),
            )
            .await
        }
        Root::Alerts(command) => alerts_cli::run(&directories, command, capture.output()).await,
    }
}

struct Usage;

impl Extension for Usage {
    async fn cli_run(
        &self,
        context: Context,
        params: CliRunParams,
    ) -> Result<CliRunResult, ErrorObject> {
        let program = match veronica_sdk::cli::parse::<Program>(&params) {
            Ok(program) => program,
            Err(result) => return Ok(result),
        };
        let capture = Capture::new(params.json);
        let outcome = run(&program, &params, &context, &capture).await;
        Ok(capture.finish(outcome))
    }
}

fn main() -> std::process::ExitCode {
    veronica_sdk::run(Usage)
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use veronica_sdk::protocol::manifest::Effect;

    use super::Program;

    const EFFECTS: [(&str, Effect); 12] = [
        ("usage summary", Effect::Read),
        ("usage sources", Effect::Read),
        ("usage models", Effect::Read),
        ("usage projects", Effect::Read),
        ("usage calendar", Effect::Read),
        ("usage limits", Effect::Read),
        ("usage alerts", Effect::Read),
        ("usage export", Effect::Write),
        ("usage refresh", Effect::Write),
        ("alerts state", Effect::Read),
        ("alerts test", Effect::Write),
        ("alerts clear", Effect::Write),
    ];

    #[test]
    fn the_manifest_lists_every_command() {
        let commands = veronica_sdk::cli::describe(Program::command(), &EFFECTS).unwrap();
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../manifest.json");
        veronica_sdk::cli::sync_manifest(&manifest, commands).unwrap();
    }

    #[test]
    fn tokens_abbreviate_with_three_significant_figures() {
        use veronica_usage::cards::tokens;
        assert_eq!(tokens(999), "999");
        assert_eq!(tokens(1_000), "1.00K");
        assert_eq!(tokens(1_050), "1.05K");
        assert_eq!(tokens(15_000), "15.0K");
        assert_eq!(tokens(502_000_260), "502M");
        assert_eq!(tokens(2_500_000_000), "2.50B");
        assert_eq!(tokens(9_999), "10.0K");
        assert_eq!(tokens(99_999), "100K");
        assert_eq!(tokens(999_999), "1.00M");
        assert_eq!(tokens(999_499), "999K");
        assert_eq!(tokens(999_999_999), "1.00B");
    }
}
