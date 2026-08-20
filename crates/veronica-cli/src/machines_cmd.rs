//! `vr machines` — the computers Veronica can reach.

use anyhow::{Context, Result};
use serde_json::json;
use veronica_core::{AppDirectories, Settings};
use veronica_machines::host::{self, Reach};
use veronica_machines::{Machine, DEFAULT_TIMEOUT};

use crate::format::{self, countdown, Output};

/// Settings key holding the stored fleet.
const MACHINES_KEY: &str = "machines";

#[derive(clap::Subcommand)]
pub enum MachineCommand {
    /// List the fleet and whether each machine answers.
    #[command(alias = "ls")]
    List {
        /// Skip probing and just show what is configured.
        #[arg(long)]
        offline: bool,
    },
    /// Add a machine reached over SSH.
    Add {
        /// The ssh target: a config alias, or user@host.
        target: String,
        /// What to call it; defaults to the target.
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        port: Option<u16>,
    },
    /// Remove a machine.
    #[command(alias = "rm")]
    Remove { id: String },
    /// Show one machine's vital signs.
    Stats {
        /// Machine id; defaults to this computer.
        #[arg(default_value = "local")]
        id: String,
    },
    /// Host aliases found in ~/.ssh/config that are not yet added.
    Discover,
}

fn load_machines(settings: &Settings) -> Vec<Machine> {
    settings
        .get(MACHINES_KEY)
        .and_then(|value| serde_json::from_value::<Vec<Machine>>(value.clone()).ok())
        .unwrap_or_default()
}

fn save_machines(
    directories: &AppDirectories,
    settings: &mut Settings,
    machines: &[Machine],
) -> Result<()> {
    settings.set(MACHINES_KEY, serde_json::to_value(machines)?);
    settings.save(&directories.settings_file())?;
    Ok(())
}

fn bytes(value: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut scaled = value as f64;
    let mut unit = 0;
    while scaled >= 1024.0 && unit + 1 < units.len() {
        scaled /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} B")
    } else if scaled >= 100.0 {
        format!("{scaled:.0} {}", units[unit])
    } else {
        format!("{scaled:.1} {}", units[unit])
    }
}

pub async fn run(
    directories: &AppDirectories,
    command: &MachineCommand,
    output: Output,
) -> Result<()> {
    let path = directories.settings_file();
    let mut settings = Settings::load(&path)?;

    match command {
        MachineCommand::List { offline } => {
            let machines = host::fleet(load_machines(&settings));
            if *offline {
                return output.emit(&machines, || {
                    let rows: Vec<Vec<String>> = machines
                        .iter()
                        .map(|machine| {
                            vec![
                                machine.id.clone(),
                                machine.name.clone(),
                                machine.ssh_target().unwrap_or("this computer").to_string(),
                            ]
                        })
                        .collect();
                    format::table(&["id", "name", "reach"], &rows)
                });
            }

            let reports = veronica_machines::probe_fleet(&machines, DEFAULT_TIMEOUT).await;
            output.emit(&reports, || {
                let rows: Vec<Vec<String>> = reports
                    .iter()
                    .map(|report| match &report.stats {
                        Some(stats) => vec![
                            report.machine.id.clone(),
                            report.machine.name.clone(),
                            format!("{:.0}%", stats.cpu_percent),
                            format!("{:.0}%", stats.memory_used_percent()),
                            stats
                                .root_disk()
                                .map(|disk| format!("{:.0}%", disk.used_percent()))
                                .unwrap_or_else(|| "—".into()),
                            countdown(stats.uptime_secs as i64),
                        ],
                        None => vec![
                            report.machine.id.clone(),
                            report.machine.name.clone(),
                            "—".into(),
                            "—".into(),
                            "—".into(),
                            "unreachable".into(),
                        ],
                    })
                    .collect();
                let mut rendered =
                    format::table(&["id", "name", "cpu", "mem", "disk", "up"], &rows);
                for report in &reports {
                    if let Some(error) = &report.error {
                        rendered.push_str(&format!("\n{}: {error}", report.machine.id));
                    }
                }
                rendered
            })
        }

        MachineCommand::Add { target, name, port } => {
            let mut machines = load_machines(&settings);
            let label = name.clone().unwrap_or_else(|| target.clone());
            let id = host::slugify(&label);
            if id == "local" {
                anyhow::bail!("\"local\" is reserved for this computer");
            }
            if machines.iter().any(|machine| machine.id == id) {
                anyhow::bail!("a machine called {id} already exists");
            }
            let machine = Machine {
                id: id.clone(),
                name: label,
                reach: Reach::Ssh {
                    target: target.clone(),
                    port: *port,
                },
            };
            machines.push(machine.clone());
            save_machines(directories, &mut settings, &machines)?;
            output.emit(&machine, || {
                format!("added {} -> ssh {}", machine.id, target)
            })
        }

        MachineCommand::Remove { id } => {
            let mut machines = load_machines(&settings);
            let before = machines.len();
            machines.retain(|machine| machine.id != *id);
            if machines.len() == before {
                anyhow::bail!("no machine called {id}");
            }
            save_machines(directories, &mut settings, &machines)?;
            output.emit(&json!({ "removed": id }), || format!("removed {id}"))
        }

        MachineCommand::Stats { id } => {
            let machines = host::fleet(load_machines(&settings));
            let machine = machines
                .iter()
                .find(|machine| machine.id == *id)
                .with_context(|| format!("no machine called {id}"))?;

            let stats = veronica_machines::probe_machine(machine, DEFAULT_TIMEOUT).await?;
            output.emit(&stats, || {
                use std::fmt::Write;
                let mut out = String::new();
                let _ = writeln!(out, "Host       {}", stats.host_name);
                let _ = writeln!(out, "OS         {}", stats.os);
                let _ = writeln!(out, "Kernel     {}", stats.kernel);
                let _ = writeln!(out, "Up         {}", countdown(stats.uptime_secs as i64));
                let _ = writeln!(out, "CPU        {:.1}%", stats.cpu_percent);
                let _ = writeln!(
                    out,
                    "Memory     {:.0}%  ({} of {})",
                    stats.memory_used_percent(),
                    bytes(stats.memory_used_bytes()),
                    bytes(stats.memory_total_bytes)
                );
                let _ = writeln!(
                    out,
                    "Load       {:.2} · {:.2} · {:.2}",
                    stats.load_average[0], stats.load_average[1], stats.load_average[2]
                );
                let rows: Vec<Vec<String>> = stats
                    .disks
                    .iter()
                    .map(|disk| {
                        vec![
                            disk.mount_point.clone(),
                            format!("{:.0}%", disk.used_percent()),
                            bytes(disk.total_bytes),
                        ]
                    })
                    .collect();
                let _ = write!(out, "{}", format::table(&["mount", "used", "size"], &rows));
                out
            })
        }

        MachineCommand::Discover => {
            let configured = load_machines(&settings);
            let config = veronica_core::paths::home_dir()
                .map(|home| home.join(".ssh/config"))
                .unwrap_or_default();
            let found = host::ssh_config_hosts(&config);
            let unknown: Vec<String> = found
                .into_iter()
                .filter(|alias| {
                    !configured
                        .iter()
                        .any(|machine| machine.ssh_target() == Some(alias.as_str()))
                })
                .collect();
            output.emit(&unknown, || {
                if unknown.is_empty() {
                    return "no new hosts in ~/.ssh/config".to_string();
                }
                let rows: Vec<Vec<String>> =
                    unknown.iter().map(|alias| vec![alias.clone()]).collect();
                format!(
                    "{}\n\nAdd one with: vr machines add <host>",
                    format::table(&["ssh host"], &rows)
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_scales_like_the_rest_of_the_interface() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1024), "1.0 KB");
        // 15078116 KiB is 14.4 GiB, which is what `free -h` reports too.
        assert_eq!(bytes(15_078_116 * 1024), "14.4 GB");
        assert_eq!(bytes(512 * 1024 * 1024 * 1024), "512 GB");
    }
}
