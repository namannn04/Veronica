//! `vr` — Veronica's command line interface.
//!
//! The Ubuntu counterpart to Edith's `ed`. It reaches the same domain
//! operations as the UI, every read command takes `--json`, stdout carries
//! exactly one document, logs go to stderr, and exit codes are meaningful, so
//! an agent can drive Veronica headlessly.

mod alerts_cmd;
mod attention_cmd;
mod audit_cmd;
mod backup_cmd;
mod calendar_cmd;
mod cleaner_cmd;
mod clipboard_cmd;
mod color_cmd;
mod companion_cmd;
mod database_cmd;
mod emoji_cmd;
mod focus_dim_cmd;
mod format;
mod herdr_cmd;
mod keystroke_cmd;
mod machines_cmd;
mod maintenance_cmd;
mod media_cmd;
mod power_cmd;
mod presenter_cmd;
mod quinjet_cmd;
mod system_cmd;
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
    /// Rate-limit alerts: the notifier's state, a delivery check and a reset.
    #[command(subcommand)]
    Alerts(alerts_cmd::AlertsCommand),
    /// Attention focus sessions and local attention history.
    #[command(subcommand)]
    Attention(attention_cmd::AttentionCommand),
    /// Export, inspect and restore Veronica's persistent data.
    #[command(subcommand)]
    Backup(backup_cmd::BackupCommand),
    /// The clipboard history.
    #[command(subcommand, alias = "clip")]
    Clipboard(clipboard_cmd::ClipboardCommand),
    /// The disk cleaner: measure developer caches and build output, and move
    /// what you choose to the Trash.
    #[command(subcommand)]
    Cleaner(cleaner_cmd::CleanerCommand),
    /// App Maintenance: what is installed, what can be updated, and removing
    /// it with the consequences shown first.
    #[command(subcommand, alias = "pkg")]
    Maintenance(maintenance_cmd::MaintenanceCommand),
    /// Site Audit: crawl a site's sitemap and check each page's metadata.
    #[command(subcommand, alias = "seo")]
    Audit(audit_cmd::AuditCommand),
    /// Explore databases and run guarded changes.
    #[command(subcommand, alias = "db")]
    Database(database_cmd::DatabaseCommand),
    /// Quinjet: discover and open review workspaces.
    #[command(subcommand)]
    Quinjet(quinjet_cmd::QuinjetCommand),
    /// Sample a colour from the screen and keep a swatch history.
    #[command(subcommand)]
    Color(color_cmd::ColorCommand),
    /// The computers Veronica can reach.
    #[command(subcommand, alias = "machine")]
    Machines(machines_cmd::MachineCommand),
    /// Your agenda, from every configured calendar.
    #[command(subcommand)]
    Calendar(calendar_cmd::CalendarCommand),
    /// Control whatever is playing, through MPRIS.
    #[command(subcommand)]
    Media(media_cmd::MediaCommand),
    /// CPU, memory, storage and battery information.
    #[command(subcommand)]
    System(system_cmd::SystemCommand),
    /// Keep Awake and Lid Awake, open-ended or for a while.
    #[command(subcommand)]
    Power(power_cmd::PowerCommand),
    /// Presenter mode: blur sensitive figures on a shared screen.
    #[command(subcommand)]
    Presenter(presenter_cmd::PresenterCommand),
    /// Focus Dim: darken everything behind the window you are working in.
    #[command(subcommand, name = "focus-dim")]
    FocusDim(focus_dim_cmd::FocusDimCommand),
    /// Keystroke Highlight: show each key press on screen for demos.
    #[command(subcommand, name = "keystroke-highlight", alias = "keys")]
    Keystroke(keystroke_cmd::KeystrokeCommand),
    /// Search the emoji catalogue and copy what you pick.
    #[command(subcommand)]
    Emoji(emoji_cmd::EmojiCommand),
    /// The live Herdr agent board.
    #[command(subcommand)]
    Herdr(herdr_cmd::HerdrCommand),
    /// Your private notes and voice-memo metadata.
    #[command(subcommand)]
    Companion(companion_cmd::CompanionCommand),
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
        Command::Alerts(command) => alerts_cmd::run(&directories, command, output).await,
        Command::Attention(command) => attention_cmd::run(&directories, command, output),
        Command::Backup(command) => backup_cmd::run(&directories, command, output),
        Command::Media(command) => media_cmd::run(command, output).await,
        Command::System(command) => system_cmd::run(command, output).await,
        Command::Power(command) => power_cmd::run(&directories, command, output).await,
        Command::Presenter(command) => presenter_cmd::run(&directories, command, output).await,
        Command::FocusDim(command) => focus_dim_cmd::run(&directories, command, output).await,
        Command::Keystroke(command) => keystroke_cmd::run(&directories, command, output).await,
        Command::Herdr(command) => herdr_cmd::run(command, output).await,
        Command::Companion(command) => companion_cmd::run(&directories, command, output),
        Command::Emoji(command) => {
            // The picker honours the configured skin tone, so the CLI and the
            // app agree on what a pick produces.
            let settings = Settings::load(&directories.settings_file())?;
            emoji_cmd::run(&directories, &settings, command, output).await
        }
        Command::Calendar(command) => calendar_cmd::run(command, output).await,
        Command::Machines(command) => machines_cmd::run(&directories, command, output).await,
        Command::Clipboard(command) => clipboard_cmd::run(&directories, command, output).await,
        Command::Cleaner(command) => cleaner_cmd::run(command, output),
        Command::Maintenance(command) => maintenance_cmd::run(command, output).await,
        Command::Audit(command) => audit_cmd::run(command, output).await,
        Command::Database(command) => database_cmd::run(&directories, command, output).await,
        Command::Quinjet(command) => quinjet_cmd::run(&directories, command, output).await,
        Command::Color(command) => {
            // The picker honours the configured format and profile, so the CLI
            // and the app agree on what a pick produces.
            let settings = Settings::load(&directories.settings_file())?;
            color_cmd::run(&directories, &settings, command, output).await
        }
    }
}

fn diagnose(directories: &AppDirectories, session: DesktopSession, output: Output) -> Result<()> {
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
        format::table(
            &["id", "title", "group", "enabled", "availability"],
            &table_rows,
        )
    })
}
