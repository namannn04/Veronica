//! `vr` — Veronica's command line interface.
//!
//! The Ubuntu counterpart to Edith's `ed`. It reaches the same domain
//! operations as the UI, every read command takes `--json`, stdout carries
//! exactly one document, logs go to stderr, and exit codes are meaningful, so
//! an agent can drive Veronica headlessly.

mod calendar_cmd;
mod format;
mod machines_cmd;
mod media_cmd;
mod usage_cmd;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use veronica_core::{AppDirectories, DesktopSession, Diagnostics, Settings};

use format::Output;

#[derive(Parser)]
#[command(
    name = "vr",
    version,
    about = "Veronica — native control center for Ubuntu",
    long_about = None,
    disable_help_subcommand = true
)]
struct Cli {
    /// Emit JSON on stdout instead of human-readable text.
    #[arg(long, global = true)]
    json: bool,

    /// Increase log verbosity on stderr.
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Report the resolved environment, capabilities and extension state.
    Diagnose,
    /// Read and write settings.
    #[command(subcommand)]
    Config(ConfigCommand),
    /// Agent usage: totals, limits, projects and collection.
    #[command(subcommand)]
    Usage(usage_cmd::UsageCommand),
    /// The computers Veronica can reach.
    #[command(subcommand, alias = "machine")]
    Machines(machines_cmd::MachineCommand),
    /// Your agenda, from every configured calendar.
    #[command(subcommand)]
    Calendar(calendar_cmd::CalendarCommand),
    /// Control whatever is playing, through MPRIS.
    #[command(subcommand)]
    Media(media_cmd::MediaCommand),
    /// List the extension catalogue and whether each one can run here.
    #[command(name = "extensions", alias = "ext")]
    Extensions {
        /// Filter by title or subtitle.
        #[arg(long, default_value = "")]
        query: String,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Print every stored setting.
    List,
    /// Print one setting.
    Get { key: String },
    /// Store one setting. The value is coerced to a JSON type.
    Set { key: String, value: String },
    /// Remove one setting, restoring its default.
    Unset { key: String },
}

fn main() {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    if let Err(error) = run(&cli) {
        // Diagnostics belong on stderr so stdout stays one clean document.
        eprintln!("vr: {error:#}");
        std::process::exit(1);
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
async fn run(cli: &Cli) -> Result<()> {
    let output = Output::new(cli.json);
    let directories = AppDirectories::current()?;
    directories.prepare()?;

    match &cli.command {
        // Probing the desktop portal needs a D-Bus round trip, so these two
        // resolve the session asynchronously rather than assuming.
        Command::Diagnose => {
            let session = veronica_system::detect_session().await;
            diagnose(&directories, session, output)
        }
        Command::Extensions { query } => {
            let session = veronica_system::detect_session().await;
            extensions(&directories, session, query, output)
        }
        Command::Config(command) => config(&directories, command, output),
        Command::Usage(command) => usage_cmd::run(&directories, command, output).await,
        Command::Media(command) => media_cmd::run(command, output).await,
        Command::Calendar(command) => calendar_cmd::run(command, output).await,
        Command::Machines(command) => machines_cmd::run(&directories, command, output).await,
    }
}

fn diagnose(
    directories: &AppDirectories,
    session: DesktopSession,
    output: Output,
) -> Result<()> {
    let settings = Settings::load(&directories.settings_file())?;
    let report = Diagnostics::collect(directories, session, &settings);

    output.emit(&report, || {
        use std::fmt::Write;
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
        let _ = writeln!(out, "{}", format::table(&["capability", "state", "backend"], &rows));

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
        let _ = write!(out, "{}", format::table(&["id", "title", "enabled", "availability"], &rows));
        out
    })
}

fn state_label(state: &veronica_core::CapabilityState) -> &'static str {
    use veronica_core::CapabilityState as S;
    match state {
        S::Available => "available",
        S::PermissionRequired { .. } => "permission",
        S::IntegrationRequired { .. } => "integration",
        S::Unsupported { .. } => "unsupported",
    }
}

fn availability_label(availability: &veronica_core::ExtensionAvailability) -> String {
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

fn extensions(
    directories: &AppDirectories,
    session: DesktopSession,
    query: &str,
    output: Output,
) -> Result<()> {
    let settings = Settings::load(&directories.settings_file())?;
    let capabilities = veronica_core::Capabilities::resolve(&session);

    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Row {
        id: &'static str,
        title: &'static str,
        subtitle: &'static str,
        group: veronica_core::ExtensionGroup,
        enabled: bool,
        #[serde(flatten)]
        availability: veronica_core::ExtensionAvailability,
    }

    let rows: Vec<Row> = veronica_core::extensions::filter(query, None)
        .into_iter()
        .map(|entry| Row {
            id: entry.id,
            title: entry.title,
            subtitle: entry.subtitle,
            group: entry.group,
            enabled: settings.extension_enabled(entry),
            availability: entry.availability(&capabilities),
        })
        .collect();

    output.emit(&rows, || {
        let table_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                vec![
                    row.id.to_string(),
                    row.title.to_string(),
                    row.group.title().to_string(),
                    if row.enabled { "on" } else { "off" }.to_string(),
                    availability_label(&row.availability),
                ]
            })
            .collect();
        format::table(&["id", "title", "group", "enabled", "availability"], &table_rows)
    })
}
