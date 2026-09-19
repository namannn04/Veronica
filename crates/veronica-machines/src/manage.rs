//! Safe interactive operations for a configured machine.
//!
//! The same SSH transport used by probes is reused here, so keys, aliases and
//! jump hosts remain the user's OpenSSH concern. Inputs are validated before a
//! short POSIX script is sent on stdin; no private credentials enter Veronica.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Serialize;
use tokio::io::AsyncWriteExt;

use crate::host::{Machine, Reach};
use crate::transport::{run_script, ssh_options};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size_bytes: u64,
    pub modified_unix: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineDirectory {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<FileEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub state: String,
    pub engine: String,
}

fn shell_quote(value: &str) -> Result<String> {
    if value.contains('\0') || value.len() > 4096 {
        bail!("path is invalid or too long");
    }
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}

fn directory_script(path: Option<&str>) -> Result<String> {
    let path = match path {
        None | Some("") | Some("~") => "$HOME".to_string(),
        Some(path) if path.starts_with('/') => shell_quote(path)?,
        Some(_) => bail!("machine paths must be absolute"),
    };
    Ok(format!(
        r#"p={path}
cd -- "$p" || exit 24
printf 'P\0%s\0' "$PWD"
find . -mindepth 1 -maxdepth 1 -printf 'E\0%y\0%s\0%T@\0%f\0' 2>/dev/null
"#
    ))
}

pub fn parse_directory(output: &str) -> Result<MachineDirectory> {
    let fields: Vec<&str> = output.split('\0').collect();
    if fields.len() < 3 || fields[0] != "P" {
        bail!("machine returned an invalid directory listing");
    }
    let path = fields[1].to_string();
    let parent = if path == "/" {
        None
    } else {
        Path::new(&path)
            .parent()
            .map(|value| value.display().to_string())
    };
    let mut entries = Vec::new();
    let mut index = 2;
    while index + 4 < fields.len() {
        if fields[index] != "E" {
            index += 1;
            continue;
        }
        let kind = match fields[index + 1] {
            "d" => "directory",
            "f" => "file",
            "l" => "link",
            _ => "other",
        };
        let name = fields[index + 4].to_string();
        if !name.is_empty() {
            entries.push(FileEntry {
                path: Path::new(&path).join(&name).display().to_string(),
                name,
                kind: kind.to_string(),
                size_bytes: fields[index + 2].parse().unwrap_or(0),
                modified_unix: fields[index + 3]
                    .split('.')
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
            });
        }
        index += 5;
    }
    entries.sort_by(|left, right| {
        let left_dir = left.kind == "directory";
        let right_dir = right.kind == "directory";
        right_dir
            .cmp(&left_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(MachineDirectory {
        path,
        parent,
        entries,
    })
}

pub async fn list_directory(
    machine: &Machine,
    path: Option<&str>,
    timeout: Duration,
) -> Result<MachineDirectory> {
    let output = run_script(machine, &directory_script(path)?, timeout).await?;
    parse_directory(&output)
}

const CONTAINERS_SCRIPT: &str = r#"
if command -v docker >/dev/null 2>&1; then engine=docker
elif command -v podman >/dev/null 2>&1; then engine=podman
else printf 'N\0'; exit 0
fi
printf 'G\0%s\0' "$engine"
"$engine" ps -a --format '{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.State}}' 2>/dev/null
"#;

pub fn parse_containers(output: &str) -> Vec<ContainerInfo> {
    let mut parts = output.splitn(3, '\0');
    if parts.next() != Some("G") {
        return Vec::new();
    }
    let engine = parts.next().unwrap_or_default().to_string();
    parts
        .next()
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            (fields.len() >= 5).then(|| ContainerInfo {
                id: fields[0].to_string(),
                name: fields[1].to_string(),
                image: fields[2].to_string(),
                status: fields[3].to_string(),
                state: fields[4].to_string(),
                engine: engine.clone(),
            })
        })
        .collect()
}

pub async fn containers(machine: &Machine, timeout: Duration) -> Result<Vec<ContainerInfo>> {
    Ok(parse_containers(
        &run_script(machine, CONTAINERS_SCRIPT, timeout).await?,
    ))
}

pub async fn container_action(
    machine: &Machine,
    engine: &str,
    container: &str,
    action: &str,
    timeout: Duration,
) -> Result<()> {
    if !matches!(engine, "docker" | "podman") || !matches!(action, "start" | "stop" | "restart") {
        bail!("unsupported container operation");
    }
    if container.is_empty()
        || !container
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ".-_".contains(ch))
    {
        bail!("invalid container id");
    }
    let script = format!(
        "command -v {engine} >/dev/null && {engine} {action} {} >/dev/null\n",
        shell_quote(container)?
    );
    run_script(machine, &script, timeout).await?;
    Ok(())
}

/// A container's recent output.
///
/// Tail rather than follow: a stream would need a channel back to the interface
/// and a way to stop it, and what the user wants when a container misbehaves is
/// the last screenful, not a live feed.
pub async fn container_logs(
    machine: &Machine,
    engine: &str,
    container: &str,
    lines: usize,
    timeout: Duration,
) -> Result<String> {
    if !matches!(engine, "docker" | "podman") {
        bail!("unsupported container engine {engine:?}");
    }
    if container.is_empty()
        || !container
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ".-_".contains(ch))
    {
        bail!("invalid container id");
    }
    // Capped so a runaway container cannot return a gigabyte over SSH.
    let lines = lines.clamp(1, 2_000);
    let script = format!(
        "command -v {engine} >/dev/null && {engine} logs --tail {lines} {} 2>&1\n",
        shell_quote(container)?
    );
    run_script(machine, &script, timeout).await
}

/// What a power action does to a machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Restart,
    Shutdown,
}

impl PowerAction {
    pub fn title(self) -> &'static str {
        match self {
            PowerAction::Restart => "restart",
            PowerAction::Shutdown => "shut down",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            PowerAction::Restart => "restart",
            PowerAction::Shutdown => "shutdown",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.to_lowercase().as_str() {
            "restart" | "reboot" => Some(PowerAction::Restart),
            "shutdown" | "poweroff" | "off" => Some(PowerAction::Shutdown),
            _ => None,
        }
    }

    /// `systemctl` is preferred because it asks logind, which handles a polkit
    /// rule the user may already have; `shutdown` is the fallback for a host
    /// without systemd.
    fn command(self) -> &'static str {
        match self {
            PowerAction::Restart => {
                "systemctl reboot 2>/dev/null || shutdown -r now 2>/dev/null || \
                 sudo -n systemctl reboot"
            }
            PowerAction::Shutdown => {
                "systemctl poweroff 2>/dev/null || shutdown -h now 2>/dev/null || \
                 sudo -n systemctl poweroff"
            }
        }
    }
}

/// Restart or shut down a machine.
///
/// Refused for the local machine: pulling the floor out from under the running
/// app is never what someone clicking a row in a fleet view meant, and Ubuntu's
/// own menu is one click away for the case where they did.
///
/// The far end decides whether it is allowed. Veronica never escalates: it asks
/// logind, falls back to `shutdown`, and finally to a *non-interactive* `sudo`,
/// which fails cleanly rather than blocking on a password prompt that has no
/// terminal to appear on.
pub async fn power(machine: &Machine, action: PowerAction, timeout: Duration) -> Result<()> {
    if machine.is_local() {
        bail!(
            "Veronica will not {} the computer it is running on; use Ubuntu's own menu",
            action.title()
        );
    }
    // The connection dies with the machine, so a closed connection is the
    // expected outcome rather than a failure worth reporting.
    match run_script(machine, action.command(), timeout).await {
        Ok(_) => Ok(()),
        Err(error) => {
            let text = error.to_string().to_lowercase();
            if text.contains("closed by remote host")
                || text.contains("connection reset")
                || text.contains("broken pipe")
            {
                Ok(())
            } else {
                Err(error).with_context(|| {
                    format!(
                        "cannot {} {}; the account Veronica connects as may not be \
                         permitted to, and Veronica never escalates on its own",
                        action.title(),
                        machine.name
                    )
                })
            }
        }
    }
}

/// Wake a machine that is powered off, with a Wake-on-LAN magic packet.
///
/// Sent as a broadcast UDP datagram, which is the whole protocol: six 0xFF
/// bytes followed by the MAC sixteen times. No SSH is involved, because the
/// machine is not answering yet — which is also why this cannot report whether
/// it worked, only that the packet went out.
pub fn wake(mac: &str) -> Result<()> {
    let bytes = parse_mac(mac)?;
    let mut packet = vec![0xFF_u8; 6];
    for _ in 0..16 {
        packet.extend_from_slice(&bytes);
    }

    let socket = std::net::UdpSocket::bind("0.0.0.0:0")
        .context("cannot open a socket to send the wake packet")?;
    socket
        .set_broadcast(true)
        .context("cannot broadcast on this network")?;
    // Port 9 is the discard port, which is where magic packets conventionally
    // go; nothing has to be listening for the NIC's firmware to see it.
    socket
        .send_to(&packet, "255.255.255.255:9")
        .context("cannot send the wake packet")?;
    Ok(())
}

/// Accept `aa:bb:cc:dd:ee:ff`, `AA-BB-CC-DD-EE-FF` and `aabbccddeeff`.
pub fn parse_mac(mac: &str) -> Result<[u8; 6]> {
    let cleaned: String = mac
        .chars()
        .filter(|character| !matches!(character, ':' | '-' | '.' | ' '))
        .collect();
    if cleaned.len() != 12 || !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("not a MAC address: {mac:?}; try aa:bb:cc:dd:ee:ff");
    }
    let mut bytes = [0_u8; 6];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&cleaned[index * 2..index * 2 + 2], 16)
            .expect("the characters were checked as hex above");
    }
    Ok(bytes)
}

pub fn open_terminal(machine: &Machine) -> Result<()> {
    let command: Vec<String> = match &machine.reach {
        Reach::Local => vec![std::env::var("SHELL").unwrap_or_else(|_| "bash".into())],
        Reach::Ssh { target, port } => {
            let mut command = vec!["ssh".to_string()];
            command.extend(ssh_options(Duration::from_secs(20)));
            if let Some(port) = port {
                command.extend(["-p".into(), port.to_string()]);
            }
            command.push(target.clone());
            command
        }
    };
    let executable = command.first().context("empty terminal command")?.clone();
    let arguments = command[1..].to_vec();
    let terminals: [(&str, Vec<String>); 3] = [
        (
            "kgx",
            std::iter::once("--".into())
                .chain(std::iter::once(executable.clone()))
                .chain(arguments.clone())
                .collect(),
        ),
        (
            "gnome-terminal",
            std::iter::once("--".into())
                .chain(std::iter::once(executable.clone()))
                .chain(arguments.clone())
                .collect(),
        ),
        (
            "x-terminal-emulator",
            std::iter::once("-e".into())
                .chain(std::iter::once(executable))
                .chain(arguments)
                .collect(),
        ),
    ];
    for (terminal, args) in terminals {
        if Command::new(terminal)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
        {
            return Ok(());
        }
    }
    bail!("no supported terminal application is installed")
}

fn sftp_quote(value: &str) -> Result<String> {
    if value.contains(['\0', '\n', '\r']) {
        bail!("file name cannot be transferred safely");
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

fn available_destination(directory: &Path, name: &str) -> PathBuf {
    let original = directory.join(name);
    if !original.exists() {
        return original;
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("download");
    let extension = path.extension().and_then(|value| value.to_str());
    for copy in 1..10_000 {
        let candidate = match extension {
            Some(extension) => directory.join(format!("{stem} ({copy}).{extension}")),
            None => directory.join(format!("{stem} ({copy})")),
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("{stem}-{}", chrono::Utc::now().timestamp_millis()))
}

pub async fn download_file(
    machine: &Machine,
    remote_path: &str,
    destination_dir: &Path,
    timeout: Duration,
) -> Result<PathBuf> {
    if !remote_path.starts_with('/') {
        bail!("remote file path must be absolute");
    }
    let name = Path::new(remote_path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .context("remote file has no usable name")?;
    std::fs::create_dir_all(destination_dir)?;
    let destination = available_destination(destination_dir, name);
    match &machine.reach {
        Reach::Local => {
            if !Path::new(remote_path).is_file() {
                bail!("selected path is not a file");
            }
            std::fs::copy(remote_path, &destination)?;
        }
        Reach::Ssh { target, port } => {
            let mut command = tokio::process::Command::new("sftp");
            command.args([
                "-q",
                "-b",
                "-",
                "-o",
                "BatchMode=yes",
                "-o",
                &format!("ConnectTimeout={}", timeout.as_secs().max(1)),
            ]);
            if let Some(port) = port {
                command.args(["-P", &port.to_string()]);
            }
            command
                .arg(target)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let mut child = command.spawn().context("cannot start sftp")?;
            if let Some(mut stdin) = child.stdin.take() {
                let batch = format!(
                    "get {} {}\n",
                    sftp_quote(remote_path)?,
                    sftp_quote(&destination.display().to_string())?
                );
                stdin.write_all(batch.as_bytes()).await?;
            }
            let output = tokio::time::timeout(timeout, child.wait_with_output())
                .await
                .context("file transfer timed out")??;
            if !output.status.success() {
                bail!(
                    "sftp failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
        }
    }
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_parser_preserves_names_and_sorts_folders_first() {
        let listing =
            "P\x00/home/n\x00E\x00f\x0012\x001700000000.0\x00z song.mp3\x00E\x00d\x004096\x001700000001.0\x00Music\x00";
        let parsed = parse_directory(listing).unwrap();
        assert_eq!(parsed.path, "/home/n");
        assert_eq!(parsed.entries[0].name, "Music");
        assert_eq!(parsed.entries[1].path, "/home/n/z song.mp3");
    }

    #[test]
    fn relative_paths_and_unsafe_container_operations_are_rejected() {
        assert!(directory_script(Some("relative")).is_err());
        assert!(shell_quote("a\0b").is_err());
    }

    #[test]
    fn container_output_is_parsed() {
        let rows = parse_containers("G\0docker\0abc\tweb\tnginx\tUp 2 hours\trunning\n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].engine, "docker");
        assert_eq!(rows[0].name, "web");
    }

    #[test]
    fn downloads_never_overwrite_an_existing_file() {
        let root =
            std::env::temp_dir().join(format!("veronica-machine-download-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("report.pdf"), b"keep").unwrap();
        assert_eq!(
            available_destination(&root, "report.pdf"),
            root.join("report (1).pdf")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn lists_the_local_home_directory_for_real() {
        let directory = list_directory(&Machine::local(), None, Duration::from_secs(5))
            .await
            .unwrap();
        assert!(directory.path.starts_with('/'));
        assert!(!directory.entries.is_empty());
    }

    #[test]
    fn a_mac_parses_in_every_form_people_write_it() {
        let expected = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        for raw in [
            "aa:bb:cc:dd:ee:ff",
            "AA-BB-CC-DD-EE-FF",
            "aabbccddeeff",
            "AA:bb:CC:dd:EE:ff",
        ] {
            assert_eq!(parse_mac(raw).unwrap(), expected, "{raw}");
        }
    }

    #[test]
    fn something_that_is_not_a_mac_is_refused_rather_than_padded() {
        for raw in [
            "",
            "aa:bb:cc",
            "aa:bb:cc:dd:ee:ff:00",
            "zz:bb:cc:dd:ee:ff",
            "hello",
        ] {
            assert!(parse_mac(raw).is_err(), "{raw:?} should not parse");
        }
    }

    #[tokio::test]
    async fn veronica_refuses_to_power_off_the_computer_it_is_running_on() {
        // Pulling the floor out from under the app is never what a click on a
        // fleet row meant.
        for action in [PowerAction::Restart, PowerAction::Shutdown] {
            let error = power(&Machine::local(), action, Duration::from_secs(1))
                .await
                .unwrap_err()
                .to_string();
            assert!(error.contains("running on"), "got {error}");
        }
    }

    #[test]
    fn a_power_action_is_named_by_what_people_type() {
        assert_eq!(PowerAction::parse("reboot"), Some(PowerAction::Restart));
        assert_eq!(PowerAction::parse("RESTART"), Some(PowerAction::Restart));
        assert_eq!(PowerAction::parse("poweroff"), Some(PowerAction::Shutdown));
        assert_eq!(PowerAction::parse("off"), Some(PowerAction::Shutdown));
        assert_eq!(PowerAction::parse("explode"), None);
    }

    #[test]
    fn a_power_command_never_escalates_interactively() {
        // A `sudo` that can prompt would block forever on a connection with no
        // terminal, and asking for a password the user never typed here would
        // be the wrong thing anyway.
        for action in [PowerAction::Restart, PowerAction::Shutdown] {
            let command = action.command();
            assert!(command.contains("sudo -n"), "got {command}");
            assert!(!command.contains("sudo systemctl"), "got {command}");
        }
    }

    #[tokio::test]
    async fn container_logs_refuse_an_unknown_engine_or_a_shell_injection() {
        let machine = Machine::local();
        let timeout = Duration::from_secs(2);
        assert!(container_logs(&machine, "rm", "abc", 10, timeout)
            .await
            .is_err());
        assert!(
            container_logs(&machine, "docker", "a; rm -rf /", 10, timeout)
                .await
                .is_err()
        );
        assert!(container_logs(&machine, "docker", "", 10, timeout)
            .await
            .is_err());
    }
}
