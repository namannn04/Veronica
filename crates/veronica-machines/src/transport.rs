//! Running the probe, locally or over SSH.
//!
//! Remote execution goes through the `ssh` binary rather than a Rust SSH
//! library, deliberately: the user's own config, keys, agent, jump hosts and
//! known-hosts all apply unchanged, so a machine that works in a terminal works
//! here with no further setup. It also means Veronica never handles a private
//! key or a passphrase.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use tokio::process::Command;

use crate::host::{Machine, Reach};
use crate::probe::{self, MachineStats};

/// How long to wait for a machine to answer before giving up.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

/// SSH options Veronica always passes.
///
/// `BatchMode` matters most: without it a host needing a password blocks
/// forever on a prompt nobody can see, which looks like a hang rather than a
/// configuration problem.
pub fn ssh_options(timeout: Duration) -> Vec<String> {
    vec![
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        format!("ConnectTimeout={}", timeout.as_secs().max(1)),
        // The probe is one short command; multiplexing would add setup cost.
        "-o".into(),
        "ControlMaster=no".into(),
        "-o".into(),
        "LogLevel=ERROR".into(),
    ]
}

/// Build the argument list for probing a machine over SSH.
pub fn ssh_args(target: &str, port: Option<u16>, script: &str, timeout: Duration) -> Vec<String> {
    let mut args = ssh_options(timeout);
    if let Some(port) = port {
        args.push("-p".into());
        args.push(port.to_string());
    }
    args.push(target.to_string());
    // The script goes to the remote shell's stdin via an argument, so quoting is
    // the remote shell's problem only once.
    args.push("sh".into());
    args.push("-s".into());
    let _ = script;
    args
}

/// Run a shell script on a machine and return its stdout.
pub async fn run_script(
    machine: &Machine,
    script: &str,
    timeout: Duration,
) -> Result<String> {
    let mut command = match &machine.reach {
        Reach::Local => {
            let mut command = Command::new("sh");
            command.arg("-s");
            command
        }
        Reach::Ssh { target, port } => {
            let mut command = Command::new("ssh");
            command.args(ssh_args(target, *port, script, timeout));
            command
        }
    };

    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("cannot reach {}", machine.name))?;

    // Feed the script on stdin so no quoting has to survive two shells.
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(script.as_bytes()).await.ok();
        stdin.shutdown().await.ok();
    }

    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .with_context(|| format!("{} did not answer within {:?}", machine.name, timeout))?
        .with_context(|| format!("cannot read from {}", machine.name))?;

    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "{} refused the probe: {}",
            machine.name,
            if detail.is_empty() {
                format!("exit status {}", output.status)
            } else {
                detail
            }
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Probe a machine for its vital signs.
pub async fn probe_machine(machine: &Machine, timeout: Duration) -> Result<MachineStats> {
    let output = run_script(machine, probe::PROBE_SCRIPT, timeout).await?;
    let stats = probe::parse(&output);
    if stats.memory_total_bytes == 0 && stats.disks.is_empty() {
        bail!(
            "{} answered but reported nothing readable; is it a Linux host?",
            machine.name
        );
    }
    Ok(stats)
}

/// One machine's probe result, for reporting a whole fleet at once.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineReport {
    pub machine: Machine,
    pub stats: Option<MachineStats>,
    /// Why the probe failed, when it did.
    pub error: Option<String>,
}

/// Probe every machine concurrently.
///
/// One unreachable host must not delay or fail the others, so each result is
/// reported independently.
pub async fn probe_fleet(machines: &[Machine], timeout: Duration) -> Vec<MachineReport> {
    let futures = machines.iter().map(|machine| async move {
        match probe_machine(machine, timeout).await {
            Ok(stats) => MachineReport {
                machine: machine.clone(),
                stats: Some(stats),
                error: None,
            },
            Err(error) => MachineReport {
                machine: machine.clone(),
                stats: None,
                error: Some(format!("{error:#}")),
            },
        }
    });
    futures_util::future::join_all(futures).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_always_runs_in_batch_mode() {
        // A password prompt nobody can see would otherwise hang the probe.
        let options = ssh_options(Duration::from_secs(10));
        assert!(options.windows(2).any(|w| w == ["-o", "BatchMode=yes"]));
        assert!(options.iter().any(|o| o == "ConnectTimeout=10"));
    }

    #[test]
    fn a_sub_second_timeout_still_produces_a_valid_connect_timeout() {
        // ssh rejects ConnectTimeout=0.
        let options = ssh_options(Duration::from_millis(200));
        assert!(options.iter().any(|o| o == "ConnectTimeout=1"));
    }

    #[test]
    fn ssh_args_include_the_target_and_a_shell() {
        let args = ssh_args("tuf", None, "echo hi", Duration::from_secs(5));
        assert!(args.contains(&"tuf".to_string()));
        // The script arrives on stdin, so a plain shell is invoked.
        assert_eq!(args.last().unwrap(), "-s");
        assert!(!args.contains(&"-p".to_string()));
    }

    #[test]
    fn a_port_is_passed_when_set() {
        let args = ssh_args("tuf", Some(2222), "echo hi", Duration::from_secs(5));
        let position = args.iter().position(|a| a == "-p").expect("-p present");
        assert_eq!(args[position + 1], "2222");
    }

    #[tokio::test]
    async fn probes_this_machine_for_real() {
        let stats = probe_machine(&Machine::local(), DEFAULT_TIMEOUT)
            .await
            .expect("the local machine should always be probeable");
        assert!(!stats.host_name.is_empty(), "hostname should be readable");
        assert!(stats.memory_total_bytes > 0, "memory should be readable");
        assert!(stats.uptime_secs > 0, "uptime should be readable");
        assert!(!stats.disks.is_empty(), "at least the root filesystem");
        assert!(
            (0.0..=100.0).contains(&stats.cpu_percent),
            "cpu out of range: {}",
            stats.cpu_percent
        );
    }

    #[tokio::test]
    async fn an_unreachable_host_reports_an_error_rather_than_hanging() {
        let machine = Machine {
            id: "nowhere".into(),
            name: "Nowhere".into(),
            reach: Reach::Ssh {
                // Reserved for documentation, so it cannot resolve to a real host.
                target: "veronica-test.invalid".into(),
                port: None,
            },
        };
        let result = probe_machine(&machine, Duration::from_secs(6)).await;
        assert!(result.is_err(), "an invalid host must fail");
    }

    #[tokio::test]
    async fn a_fleet_probe_reports_each_machine_independently() {
        let machines = vec![
            Machine::local(),
            Machine {
                id: "nowhere".into(),
                name: "Nowhere".into(),
                reach: Reach::Ssh {
                    target: "veronica-test.invalid".into(),
                    port: None,
                },
            },
        ];
        let reports = probe_fleet(&machines, Duration::from_secs(6)).await;
        assert_eq!(reports.len(), 2);
        assert!(reports[0].stats.is_some(), "the local machine should answer");
        assert!(reports[0].error.is_none());
        assert!(reports[1].stats.is_none(), "the invalid host should not");
        assert!(reports[1].error.is_some(), "and should say why");
    }
}
