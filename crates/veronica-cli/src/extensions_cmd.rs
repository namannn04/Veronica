//! `vr extensions` — the catalogue in front of each feature's shared switch.
//!
//! These are separate from `vr config` because the catalogue can explain what
//! a key controls and what setup remains. Enabling is deliberately
//! noninteractive: the switch is written even when a capability or tool is not
//! ready, and the shortfall is reported afterwards.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use veronica_core::{
    AppDirectories, Capabilities, Capability, CapabilityState, DesktopSession,
    ExtensionAvailability, ExtensionEntry, ExtensionGroup, Settings,
};
use veronica_system::tools::Report as ToolReport;

use crate::format::{self, Output};

#[derive(clap::Subcommand)]
pub enum ExtensionCommand {
    /// Every extension, its group, switch and availability.
    #[command(alias = "list")]
    Ls,
    /// Turn one extension on, even when setup still remains.
    Enable { id: String },
    /// Turn one extension off.
    Disable { id: String },
    /// Describe one extension and everything it needs.
    Info { id: String },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    id: &'static str,
    title: &'static str,
    subtitle: &'static str,
    group: ExtensionGroup,
    enabled: bool,
    #[serde(flatten)]
    availability: ExtensionAvailability,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CapabilityReport {
    capability: Capability,
    title: &'static str,
    backend: &'static str,
    #[serde(flatten)]
    state: CapabilityState,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    id: &'static str,
    title: &'static str,
    subtitle: &'static str,
    settings_key: &'static str,
    group: ExtensionGroup,
    enabled: bool,
    #[serde(flatten)]
    availability: ExtensionAvailability,
    required_capabilities: Vec<CapabilityReport>,
    optional_capabilities: Vec<CapabilityReport>,
    required_tools: Vec<ToolReport>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Mutation {
    id: &'static str,
    enabled: bool,
    #[serde(flatten)]
    availability: ExtensionAvailability,
    missing_tools: Vec<&'static str>,
}

pub async fn run(
    directories: &AppDirectories,
    session: DesktopSession,
    command: &ExtensionCommand,
    query: &str,
    output: Output,
) -> Result<()> {
    let capabilities = Capabilities::resolve(&session);
    match command {
        ExtensionCommand::Ls => list(directories, &capabilities, query, output),
        ExtensionCommand::Enable { id } => {
            mutate(directories, &capabilities, id, true, output).await
        }
        ExtensionCommand::Disable { id } => {
            mutate(directories, &capabilities, id, false, output).await
        }
        ExtensionCommand::Info { id } => info(directories, &capabilities, id, output).await,
    }
}

fn list(
    directories: &AppDirectories,
    capabilities: &Capabilities,
    query: &str,
    output: Output,
) -> Result<()> {
    let settings = Settings::load(&directories.settings_file())?;
    let rows: Vec<Row> = veronica_core::extensions::filter(query, None)
        .into_iter()
        .map(|entry| Row {
            id: entry.id,
            title: entry.title,
            subtitle: entry.subtitle,
            group: entry.group,
            enabled: settings.extension_enabled(entry),
            availability: entry.availability(capabilities),
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
                    crate::availability_label(&row.availability),
                ]
            })
            .collect();
        format::table(
            &["id", "title", "group", "enabled", "availability"],
            &table_rows,
        )
    })
}

async fn mutate(
    directories: &AppDirectories,
    capabilities: &Capabilities,
    id: &str,
    enabled: bool,
    output: Output,
) -> Result<()> {
    let entry = find(id)?;
    write_switch(&directories.settings_file(), entry, enabled)?;

    let availability = entry.availability(capabilities);
    let missing_tools: Vec<&'static str> = if enabled && !entry.required_tools.is_empty() {
        let reports = veronica_system::tools::catalogue().await;
        veronica_system::tools::unmet(entry, &reports)
            .into_iter()
            .map(|tool| tool.id)
            .collect()
    } else {
        Vec::new()
    };

    if enabled {
        warn_shortfalls(&availability, &missing_tools);
    }

    let result = Mutation {
        id: entry.id,
        enabled,
        availability,
        missing_tools,
    };
    output.emit(&result, || {
        if enabled {
            format!("enabled {}", entry.title)
        } else {
            format!("disabled {}", entry.title)
        }
    })
}

async fn info(
    directories: &AppDirectories,
    capabilities: &Capabilities,
    id: &str,
    output: Output,
) -> Result<()> {
    let entry = find(id)?;
    let settings = Settings::load(&directories.settings_file())?;
    let tools = veronica_system::tools::catalogue().await;
    let required_tools = entry
        .required_tools
        .iter()
        .filter_map(|id| tools.iter().find(|report| report.id == *id).cloned())
        .collect();
    let report = Info {
        id: entry.id,
        title: entry.title,
        subtitle: entry.subtitle,
        settings_key: entry.defaults_key,
        group: entry.group,
        enabled: settings.extension_enabled(entry),
        availability: entry.availability(capabilities),
        required_capabilities: capability_reports(entry.required_capabilities, capabilities),
        optional_capabilities: capability_reports(entry.optional_capabilities, capabilities),
        required_tools,
    };

    output.emit(&report, || render_info(&report))
}

fn capability_reports(
    requested: &[Capability],
    capabilities: &Capabilities,
) -> Vec<CapabilityReport> {
    requested
        .iter()
        .map(|capability| CapabilityReport {
            capability: *capability,
            title: capability.title(),
            backend: capability.backend(),
            state: capabilities.state(*capability).clone(),
        })
        .collect()
}

fn render_info(report: &Info) -> String {
    use std::fmt::Write;

    let mut out = format!(
        "{} ({})\n{}\n\nsetting       {}\ngroup         {}\nenabled       {}\navailability  {}",
        report.title,
        report.id,
        report.subtitle,
        report.settings_key,
        report.group.title(),
        if report.enabled { "on" } else { "off" },
        crate::availability_label(&report.availability),
    );
    for (heading, capabilities) in [
        ("Required capabilities", &report.required_capabilities),
        ("Optional capabilities", &report.optional_capabilities),
    ] {
        if capabilities.is_empty() {
            continue;
        }
        let rows: Vec<Vec<String>> = capabilities
            .iter()
            .map(|capability| {
                vec![
                    capability.title.to_string(),
                    crate::state_label(&capability.state).to_string(),
                    capability.backend.to_string(),
                ]
            })
            .collect();
        let _ = write!(
            out,
            "\n\n{heading}\n{}",
            format::table(&["capability", "state", "backend"], &rows)
        );
    }
    if !report.required_tools.is_empty() {
        let rows: Vec<Vec<String>> = report
            .required_tools
            .iter()
            .map(|tool| vec![tool.id.to_string(), tool.readiness.summary()])
            .collect();
        let _ = write!(
            out,
            "\n\nRequired tools\n{}",
            format::table(&["tool", "state"], &rows)
        );
    }
    out
}

fn find(id: &str) -> Result<&'static ExtensionEntry> {
    veronica_core::extensions::entry(id).with_context(|| {
        let known = veronica_core::extensions::ENTRIES
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>()
            .join(", ");
        format!("no extension called '{id}'; try one of {known}")
    })
}

fn write_switch(path: &Path, entry: &ExtensionEntry, enabled: bool) -> Result<()> {
    let mut settings = Settings::load(path)?;
    settings.set(entry.defaults_key, serde_json::Value::Bool(enabled));
    settings.save(path)
}

fn warn_shortfalls(availability: &ExtensionAvailability, missing_tools: &[&str]) {
    let missing_capabilities = match availability {
        ExtensionAvailability::Available => &[][..],
        ExtensionAvailability::Degraded { missing }
        | ExtensionAvailability::Unavailable { missing } => missing,
    };
    if !missing_capabilities.is_empty() {
        eprintln!(
            "still needs capabilities: {}",
            missing_capabilities
                .iter()
                .map(|capability| capability.title())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if !missing_tools.is_empty() {
        eprintln!("still needs tools: {}", missing_tools.join(", "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_id_names_the_real_choices() {
        let error = find("not-real").unwrap_err().to_string();
        assert!(error.contains("no extension called 'not-real'"));
        assert!(error.contains("attention, usage, herdr"));
        assert!(error.contains("colorPicker"));
    }

    #[test]
    fn enable_and_disable_write_the_catalogues_shared_key() {
        let directory = std::env::temp_dir().join(format!(
            "veronica-extension-switch-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("settings.json");
        let entry = find("herdr").unwrap();

        write_switch(&path, entry, true).unwrap();
        assert!(Settings::load(&path).unwrap().extension_enabled(entry));

        write_switch(&path, entry, false).unwrap();
        assert!(!Settings::load(&path).unwrap().extension_enabled(entry));
        std::fs::remove_dir_all(directory).ok();
    }

    #[test]
    fn capability_rows_keep_the_resolved_state_and_backend() {
        let capabilities = Capabilities::resolve(&DesktopSession::unknown());
        let rows = capability_reports(&[Capability::UsageCollection], &capabilities);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Usage collection");
        assert_eq!(rows[0].backend, "Filesystem");
        assert_eq!(rows[0].state, CapabilityState::Available);
    }
}
