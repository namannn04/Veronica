use std::fmt::Write as _;

mod alerts_cmd;
mod attention_cmd;
mod backup_cmd;
mod cleaner_cmd;
mod clipboard_cmd;
mod color_cmd;
mod companion_cmd;
mod database_cmd;
mod emoji_cmd;
mod extensions_cmd;
mod focus_dim_cmd;
mod format;
mod guide_cmd;
mod herdr_cmd;
mod invoke_cmd;
mod keystroke_cmd;
mod machines_cmd;
mod maintenance_cmd;
mod mcp_cmd;
mod power_cmd;
mod presenter_cmd;
mod quinjet_cmd;
mod routes;
mod services_cmd;
mod shelf_cmd;
mod surfaces_cmd;
mod system_cmd;
mod tools_cmd;
mod usage_cmd;
mod utility_cmd;
mod workspace_cmd;

use anyhow::{Context, Result};
use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser, Subcommand};
use veronica_core::{AppDirectories, DesktopSession, Diagnostics, Settings};
use veronica_protocol::ExitCode;

use format::Output;

#[derive(Parser)]
#[command(
    name = "vr",
    version,
    about = "Veronica: native control center for Ubuntu",
    long_about = None,
    disable_help_subcommand = true
)]
struct Cli {
    #[arg(
        long,
        global = true,
        help = "Emit JSON on stdout instead of human-readable text"
    )]
    json: bool,

    #[arg(
        short,
        long,
        global = true,
        help = "Confirm destructive commands instead of printing a preview"
    )]
    yes: bool,

    #[arg(short, long, global = true, action = clap::ArgAction::Count, help = "Increase log verbosity on stderr")]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(subcommand, about = "Local notch image conversion and QR generation")]
    Utility(utility_cmd::UtilityCommand),
    #[command(subcommand, about = "On-demand, read-only developer project checks")]
    Workspace(workspace_cmd::WorkspaceCommand),
    #[command(
        subcommand,
        about = "Inspect local TCP/UDP ports and gracefully stop owned processes"
    )]
    Ports(services_cmd::PortsCommand),
    #[command(
        subcommand,
        about = "Local Docker status, containers and published ports"
    )]
    Docker(services_cmd::DockerCommand),
    #[command(about = "Report the resolved environment, capabilities and extension state")]
    Diagnose,
    #[command(about = "The manual, for people and agents; with --json, every command as data")]
    Guide {
        #[arg(value_enum)]
        topic: Option<guide_cmd::Topic>,
    },
    #[command(about = "List the Home cards and notch activities extensions are showing")]
    Surfaces,
    #[command(about = "Call one method on an installed extension's backend")]
    Invoke {
        id: String,
        #[arg(help = "The call as JSON: {\"method\": \"...\", \"params\": {}}")]
        request: String,
    },
    #[command(subcommand, about = "Read and write settings")]
    Config(ConfigCommand),
    #[command(
        subcommand,
        about = "Agent usage: totals, limits, projects and collection"
    )]
    Usage(usage_cmd::UsageCommand),
    #[command(
        subcommand,
        about = "Rate-limit alerts: the notifier's state, a delivery check and a reset"
    )]
    Alerts(alerts_cmd::AlertsCommand),
    #[command(
        subcommand,
        about = "Attention focus sessions and local attention history"
    )]
    Attention(attention_cmd::AttentionCommand),
    #[command(
        subcommand,
        about = "Export, inspect and restore Veronica's persistent data"
    )]
    Backup(backup_cmd::BackupCommand),
    #[command(subcommand, alias = "clip", about = "The clipboard history")]
    Clipboard(clipboard_cmd::ClipboardCommand),
    #[command(
        subcommand,
        about = "The disk cleaner: measure developer caches and build output, and move what you choose to the Trash"
    )]
    Cleaner(cleaner_cmd::CleanerCommand),
    #[command(
        subcommand,
        alias = "pkg",
        about = "App Maintenance: what is installed, what can be updated, and removing it with the consequences shown first"
    )]
    Maintenance(maintenance_cmd::MaintenanceCommand),
    #[command(
        subcommand,
        alias = "db",
        about = "Explore databases and run guarded changes"
    )]
    Database(database_cmd::DatabaseCommand),
    #[command(subcommand, about = "Quinjet: discover and open review workspaces")]
    Quinjet(quinjet_cmd::QuinjetCommand),
    #[command(
        subcommand,
        about = "Sample a colour from the screen and keep a swatch history"
    )]
    Color(color_cmd::ColorCommand),
    #[command(
        subcommand,
        alias = "machine",
        about = "The computers Veronica can reach"
    )]
    Machines(machines_cmd::MachineCommand),
    #[command(subcommand, about = "CPU, memory, storage and battery information")]
    System(system_cmd::SystemCommand),
    #[command(
        subcommand,
        about = "Keep Awake and Lid Awake, open-ended or for a while"
    )]
    Power(power_cmd::PowerCommand),
    #[command(
        subcommand,
        about = "Presenter mode: blur sensitive figures on a shared screen"
    )]
    Presenter(presenter_cmd::PresenterCommand),
    #[command(
        subcommand,
        name = "focus-dim",
        about = "Focus Dim: darken everything behind the window you are working in"
    )]
    FocusDim(focus_dim_cmd::FocusDimCommand),
    #[command(
        subcommand,
        name = "keystroke-highlight",
        alias = "keys",
        about = "Keystroke Highlight: show each key press on screen for demos"
    )]
    Keystroke(keystroke_cmd::KeystrokeCommand),
    #[command(
        subcommand,
        about = "Search the emoji catalogue and copy what you pick"
    )]
    Emoji(emoji_cmd::EmojiCommand),
    #[command(subcommand, about = "The live Herdr agent board")]
    Herdr(herdr_cmd::HerdrCommand),
    #[command(
        subcommand,
        about = "Owned file copies shared by the notch and native drop shelf"
    )]
    Shelf(shelf_cmd::ShelfCommand),
    #[command(subcommand, about = "Your private notes and voice-memo metadata")]
    Companion(companion_cmd::CompanionCommand),
    #[command(
        about = "The command line programs Veronica's extensions need, and whether this computer has them"
    )]
    Tools {
        #[command(subcommand)]
        command: Option<tools_cmd::ToolCommand>,
    },
    #[command(
        about = "Give agents Veronica's commands over MCP, and register the server with agent clients"
    )]
    Mcp {
        #[command(subcommand)]
        command: Option<mcp_cmd::McpCommand>,
    },
    #[command(
        name = "extensions",
        alias = "ext",
        about = "Install, update, inspect and switch Veronica's extensions"
    )]
    Extensions {
        #[command(subcommand)]
        command: Option<extensions_cmd::ExtensionCommand>,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    #[command(about = "Print every stored setting")]
    List,
    #[command(about = "Print one setting")]
    Get { key: String },
    #[command(about = "Store one setting. The value is coerced to a JSON type")]
    Set { key: String, value: String },
    #[command(about = "Remove one setting, restoring its default")]
    Unset { key: String },
}

fn main() {
    let arguments = routes::arguments();
    let mut routes = routes::Routes::discover();
    let program = routes.command(Cli::command());
    let code = match program.try_get_matches_from(std::env::args_os()) {
        Ok(matches) => {
            let json = matches.get_flag("json");
            init_logging(matches.get_count("verbose"));
            match matches
                .subcommand_name()
                .and_then(|name| routes.route(name))
            {
                Some(route) => {
                    let yes = matches.get_flag("yes");
                    routes::dispatch(route, &matches, &arguments, json, yes)
                }
                None => host(&matches, json, &routes),
            }
        }
        Err(error) => routes.reject(&error, &arguments),
    };
    std::process::exit(i32::from(code));
}

fn host(matches: &ArgMatches, json: bool, routes: &routes::Routes) -> ExitCode {
    let cli = Cli::from_arg_matches(matches).unwrap_or_else(|error| error.exit());
    let path = guide_cmd::path(matches);
    if !cli.yes && guide_cmd::needs_confirmation(&path) {
        let preview = serde_json::json!({ "command": path, "effect": "destructive" });
        let error = format::confirmation(Output::new(json), &preview, || {
            format!("`vr {path}` removes or replaces data.")
        });
        return format::fail(json, ExitCode::ConfirmationRequired, &format!("{error:#}"));
    }
    match run(&cli, routes) {
        Ok(()) => ExitCode::Success,
        Err(error) => {
            let code = error
                .downcast_ref::<format::Failure>()
                .map_or(ExitCode::Failed, |failure| failure.code);
            format::fail(json, code, &format!("{error:#}"))
        }
    }
}

fn init_logging(verbosity: u8) {
    let level = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = std::env::var("VERONICA_LOG").unwrap_or_else(|_| level.to_string());
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .without_time()
        .init();
}

#[tokio::main(flavor = "current_thread")]
async fn run(cli: &Cli, routes: &routes::Routes) -> Result<()> {
    let output = Output::new(cli.json);
    let directories = AppDirectories::current()?;
    directories.prepare()?;

    match &cli.command {
        Command::Guide { topic } => guide_cmd::run(routes, *topic, output),
        Command::Diagnose => {
            let session = veronica_system::detect_session().await;
            diagnose(&directories, session, output)
        }
        Command::Extensions { command } => {
            extensions_cmd::run(
                directories.clone(),
                command
                    .as_ref()
                    .unwrap_or(&extensions_cmd::ExtensionCommand::Ls),
                output,
                cli.yes,
            )
            .await
        }
        Command::Mcp { command } => {
            mcp_cmd::run(
                command.as_ref().unwrap_or(&mcp_cmd::McpCommand::Serve),
                output,
                cli.yes,
            )
            .await
        }
        Command::Invoke { id, request } => invoke_cmd::run(id, request, output).await,
        Command::Surfaces => surfaces_cmd::run(&directories, output),
        Command::Tools { command } => {
            tools_cmd::run(
                command.as_ref().unwrap_or(&tools_cmd::ToolCommand::Ls),
                output,
                cli.yes,
            )
            .await
        }
        Command::Config(command) => config(&directories, command, output),
        Command::Shelf(command) => shelf_cmd::run(&directories, command, output),
        Command::Usage(command) => usage_cmd::run(&directories, command, output).await,
        Command::Alerts(command) => alerts_cmd::run(&directories, command, output).await,
        Command::Attention(command) => attention_cmd::run(&directories, command, output),
        Command::Backup(command) => backup_cmd::run(&directories, command, output),
        Command::Workspace(command) => workspace_cmd::run(command, output).await,
        Command::Utility(command) => utility_cmd::run(&directories, command, output),
        Command::Ports(command) => services_cmd::ports(command, output).await,
        Command::Docker(command) => services_cmd::docker(command, output).await,
        Command::System(command) => system_cmd::run(command, output).await,
        Command::Power(command) => power_cmd::run(&directories, command, output),
        Command::Presenter(command) => presenter_cmd::run(&directories, command, output).await,
        Command::FocusDim(command) => focus_dim_cmd::run(&directories, command, output).await,
        Command::Keystroke(command) => keystroke_cmd::run(&directories, command, output).await,
        Command::Herdr(command) => herdr_cmd::run(command, output).await,
        Command::Companion(command) => companion_cmd::run(&directories, command, output),
        Command::Emoji(command) => {
            let settings = Settings::load(&directories.settings_file())?;
            emoji_cmd::run(&directories, &settings, command, output).await
        }
        Command::Machines(command) => machines_cmd::run(&directories, command, output).await,
        Command::Clipboard(command) => clipboard_cmd::run(&directories, command, output),
        Command::Cleaner(command) => cleaner_cmd::run(command, output, cli.yes),
        Command::Maintenance(command) => maintenance_cmd::run(command, output, cli.yes).await,
        Command::Database(command) => database_cmd::run(&directories, command, output).await,
        Command::Quinjet(command) => quinjet_cmd::run(&directories, command, output).await,
        Command::Color(command) => {
            let settings = Settings::load(&directories.settings_file())?;
            color_cmd::run(&directories, &settings, command, output).await
        }
    }
}

fn diagnose(directories: &AppDirectories, session: DesktopSession, output: Output) -> Result<()> {
    let settings = Settings::load(&directories.settings_file())?;
    let report = Diagnostics::collect(directories, session, &settings);

    output.emit(&report, || {
        let mut out = String::new();
        let _ = writeln!(out, "Veronica {}", report.version);
        let _ = writeln!(
            out,
            "Session   {:?} on {}",
            report.session.kind, report.session.desktop
        );
        let _ = writeln!(out, "Config    {}", report.directories.configuration);
        let _ = writeln!(out, "Data      {}", report.directories.data);
        let _ = writeln!(out, "Cache     {}", report.directories.cache);
        let _ = writeln!(out, "State     {}", report.directories.state);
        let _ = writeln!(out, "Runtime   {}", report.directories.runtime);

        let _ = writeln!(out, "\nCapabilities");
        let rows: Vec<Vec<String>> = veronica_core::Capability::ALL
            .iter()
            .map(|capability| {
                let state = report.capabilities.state(*capability);
                vec![
                    capability.title().to_string(),
                    state_label(state).to_string(),
                    capability.backend().to_string(),
                ]
            })
            .collect();
        let _ = writeln!(
            out,
            "{}",
            format::table(&["capability", "state", "backend"], &rows)
        );

        let _ = writeln!(out, "\nExtensions");
        let rows: Vec<Vec<String>> = report
            .extensions
            .iter()
            .map(|entry| {
                vec![
                    entry.id.to_string(),
                    entry.title.to_string(),
                    if entry.enabled { "on" } else { "off" }.to_string(),
                    availability_label(&entry.availability),
                ]
            })
            .collect();
        let _ = write!(
            out,
            "{}",
            format::table(&["id", "title", "enabled", "availability"], &rows)
        );
        out
    })
}

pub(crate) fn state_label(state: &veronica_core::CapabilityState) -> &'static str {
    use veronica_core::CapabilityState as S;
    match state {
        S::Available => "available",
        S::PermissionRequired { .. } => "permission",
        S::IntegrationRequired { .. } => "integration",
        S::Unsupported { .. } => "unsupported",
    }
}

pub(crate) fn availability_label(availability: &veronica_core::ExtensionAvailability) -> String {
    use veronica_core::ExtensionAvailability as A;
    match availability {
        A::Available => "available".to_string(),
        A::Degraded { missing } => format!("degraded ({} missing)", missing.len()),
        A::Unavailable { missing } => {
            let names: Vec<&str> = missing.iter().map(|c| c.title()).collect();
            format!("unavailable: needs {}", names.join(", "))
        }
    }
}

fn config(directories: &AppDirectories, command: &ConfigCommand, output: Output) -> Result<()> {
    let path = directories.settings_file();
    let mut settings = Settings::load(&path)?;

    match command {
        ConfigCommand::List => output.emit(settings.as_map(), || {
            let rows: Vec<Vec<String>> = settings
                .as_map()
                .iter()
                .map(|(key, value)| vec![key.clone(), value.to_string()])
                .collect();
            format::table(&["key", "value"], &rows)
        }),
        ConfigCommand::Get { key } => {
            let value = settings
                .get(key)
                .cloned()
                .with_context(|| format!("{key} is not set"))?;
            output.emit(&value, || value.to_string())
        }
        ConfigCommand::Set { key, value } => {
            let coerced = Settings::coerce(value);
            settings.set(key, coerced.clone());
            settings.save(&path)?;
            output.emit(&serde_json::json!({ key: coerced }), || {
                format!("{key} = {coerced}")
            })
        }
        ConfigCommand::Unset { key } => {
            let removed = settings.remove(key);
            settings.save(&path)?;
            output.emit(&serde_json::json!({ "removed": removed.is_some() }), || {
                if removed.is_some() {
                    format!("{key} unset")
                } else {
                    format!("{key} was not set")
                }
            })
        }
    }
}
