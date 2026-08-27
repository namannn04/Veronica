use anyhow::Result;
use clap::Subcommand;

use crate::format::Output;

#[derive(Subcommand)]
pub enum SystemCommand {
    /// Read CPU, memory, disks, temperatures and battery state.
    Snapshot,
}

pub fn run(command: &SystemCommand, output: Output) -> Result<()> {
    match command {
        SystemCommand::Snapshot => {
            let snapshot = veronica_system::MetricsSampler::new().sample();
            output.emit(&snapshot, || {
                let memory = snapshot.memory.used_percent();
                format!(
                    "CPU {:.0}% · memory {:.0}% · load {:.2}",
                    snapshot.cpu.usage_percent, memory, snapshot.load_average[0]
                )
            })
        }
    }
}
