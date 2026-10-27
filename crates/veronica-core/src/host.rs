//! Running the computer's own programs, from inside a Flatpak sandbox or not.
//!
//! Veronica is a control center for the machine it runs on: it asks PipeWire,
//! GNOME Shell, apt, ssh and the user's coding agents for their state. In the
//! Debian package those programs are simply on PATH. Inside the Flatpak they
//! are not — the sandbox has the GNOME runtime's /usr, not the host's — so every
//! such call is routed through `flatpak-spawn --host` instead, and executables
//! are looked for where the host keeps them rather than where the sandbox does.
//!
//! Outside a sandbox every function here is exactly the plain std equivalent,
//! so the Debian package and the AppImage behave as they always have.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Where the host's operating system files appear inside the sandbox, given the
/// `--filesystem=host-os` permission the Flatpak manifest grants.
const HOST_OS_MOUNT: &str = "/run/host";

/// The host prefixes that `host-os` exposes under [`HOST_OS_MOUNT`]. Anything
/// else (the home directory, /snap) is either shared at its own path or not at
/// all.
const HOST_OS_PREFIXES: [&str; 5] = ["/usr", "/bin", "/sbin", "/lib", "/etc"];

/// True when this process runs inside a Flatpak sandbox.
pub fn is_sandboxed() -> bool {
    static SANDBOXED: OnceLock<bool> = OnceLock::new();
    *SANDBOXED.get_or_init(|| Path::new("/.flatpak-info").exists())
}

/// Start a program that outlives the call, without leaving a zombie.
///
/// A child that is spawned and dropped stays in the process table as a zombie
/// once it exits, until Veronica itself exits: every "Open in browser",
/// terminal and calendar launch added one. A thread waits for it instead.
pub fn spawn_detached(command: &mut Command) -> std::io::Result<u32> {
    let mut child = command.spawn()?;
    let id = child.id();
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(id)
}

/// A command that runs `program` on the host.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    command_with_env(program, &[])
}

/// A command that runs `program` on the host with extra environment variables.
///
/// `flatpak-spawn` does not forward the environment of its own process, so a
/// variable set with [`Command::env`] would never reach a host program. Every
/// variable the host program needs is therefore named up front.
pub fn command_with_env(program: impl AsRef<OsStr>, env: &[(&str, &OsStr)]) -> Command {
    if !is_sandboxed() {
        let mut command = Command::new(program);
        command.envs(env.iter().copied());
        return command;
    }
    let mut command = Command::new("flatpak-spawn");
    command.arg("--host");
    for (key, value) in env {
        let mut assignment = std::ffi::OsString::from(format!("--env={key}="));
        assignment.push(value);
        command.arg(assignment);
    }
    command.arg(program);
    command
}

/// Where a host path can be read from this process.
///
/// Inside the sandbox the host's /usr is mounted under /run/host; the home
/// directory is shared at its own path. Outside, every path is already the
/// host's.
pub fn visible(path: &Path) -> PathBuf {
    if !is_sandboxed() {
        return path.to_path_buf();
    }
    mapped(path)
}

fn mapped(path: &Path) -> PathBuf {
    let under_host_os = HOST_OS_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix));
    if under_host_os {
        Path::new(HOST_OS_MOUNT).join(path.strip_prefix("/").unwrap_or(path))
    } else {
        path.to_path_buf()
    }
}

/// The host's executable search path.
///
/// Inside the sandbox PATH describes the sandbox, so the host's is rebuilt from
/// the standard system directories; callers that also search per-user
/// directories add those themselves.
pub fn path_directories() -> Vec<PathBuf> {
    if is_sandboxed() {
        return [
            "/usr/local/sbin",
            "/usr/local/bin",
            "/usr/sbin",
            "/usr/bin",
            "/sbin",
            "/bin",
            "/snap/bin",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default()
}

/// The first host executable called `binary` on the host's search path, as the
/// host names it.
pub fn which(binary: &str) -> Option<PathBuf> {
    path_directories()
        .into_iter()
        .map(|dir| dir.join(binary))
        .find(|candidate| is_executable(candidate))
}

/// Whether the host file at `path` is an executable file.
pub fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(visible(path))
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_detached_child_is_reaped_when_it_exits() {
        let id = spawn_detached(&mut Command::new("true")).unwrap();
        let stat = format!("/proc/{id}/stat");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        // Gone from the process table, rather than lingering as a zombie.
        while std::path::Path::new(&stat).exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "child {id} was not reaped"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn system_paths_are_read_through_the_host_mount() {
        assert_eq!(
            mapped(Path::new("/usr/bin/wpctl")),
            PathBuf::from("/run/host/usr/bin/wpctl")
        );
        assert_eq!(
            mapped(Path::new("/bin/sh")),
            PathBuf::from("/run/host/bin/sh")
        );
    }

    #[test]
    fn home_paths_are_shared_at_their_own_path() {
        assert_eq!(
            mapped(Path::new("/home/someone/.local/bin/codex")),
            PathBuf::from("/home/someone/.local/bin/codex")
        );
        // A prefix match is by component, not by string.
        assert_eq!(
            mapped(Path::new("/usrlocal/bin/x")),
            PathBuf::from("/usrlocal/bin/x")
        );
    }

    #[test]
    fn outside_a_sandbox_a_command_is_the_program_itself() {
        if is_sandboxed() {
            return;
        }
        let command = command_with_env("wpctl", &[("LC_ALL", OsStr::new("C"))]);
        assert_eq!(command.get_program(), "wpctl");
        assert!(command
            .get_envs()
            .any(|(key, value)| key == "LC_ALL" && value == Some(OsStr::new("C"))));
    }
}
