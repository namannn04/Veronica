use std::fmt::Write as _;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use veronica_core::{
    Capabilities, Capability, CapabilityState, ExtensionAvailability, ExtensionEntry,
    ExtensionGroup, Settings,
};
use veronica_system::tools::Report as ToolReport;

use crate::format::{self, Output};

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
    origin: &'static str,
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
    origin: &'static str,
    enabled: bool,
    #[serde(flatten)]
    availability: ExtensionAvailability,
    missing_tools: Vec<&'static str>,
}

pub(super) async fn switch(
    settings_file: &Path,
    capabilities: &Capabilities,
    entry: &'static ExtensionEntry,
    enabled: bool,
    output: Output,
) -> Result<()> {
    write_switch(settings_file, entry, enabled)?;
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
        origin: "builtin",
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

pub(super) async fn info(
    settings: &Settings,
    capabilities: &Capabilities,
    entry: &'static ExtensionEntry,
    output: Output,
) -> Result<()> {
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
        origin: "builtin",
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
    let mut out = format!(
        "{} ({})\n{}\n\norigin        built in\nsetting       {}\ngroup         {}\nenabled       {}\navailability  {}",
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
    use veronica_core::DesktopSession;

    use super::*;

    #[test]
    fn enable_and_disable_write_the_catalogues_shared_key() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let entry = veronica_core::extensions::entry("herdr").unwrap();

        write_switch(&path, entry, true).unwrap();
        assert!(Settings::load(&path).unwrap().extension_enabled(entry));

        write_switch(&path, entry, false).unwrap();
        assert!(!Settings::load(&path).unwrap().extension_enabled(entry));
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
