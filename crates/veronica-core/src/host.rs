use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

const HOST_OS_MOUNT: &str = "/run/host";

const HOST_OS_PREFIXES: [&str; 5] = ["/usr", "/bin", "/sbin", "/lib", "/etc"];

pub fn is_sandboxed() -> bool {
    static SANDBOXED: OnceLock<bool> = OnceLock::new();
    *SANDBOXED.get_or_init(|| Path::new("/.flatpak-info").exists())
}

pub fn spawn_detached(command: &mut Command) -> std::io::Result<u32> {
    let mut child = command.spawn()?;
    let id = child.id();
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(id)
}

pub fn command(program: impl AsRef<OsStr>) -> Command {
    command_with_env(program, &[])
}

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

pub fn which(binary: &str) -> Option<PathBuf> {
    path_directories()
        .into_iter()
        .map(|dir| dir.join(binary))
        .find(|candidate| is_executable(candidate))
}

pub fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(visible(path))
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_detached_child_is_reaped_when_it_exits() {
        let id = spawn_detached(&mut Command::new("true")).unwrap();
        let stat = format!("/proc/{id}/stat");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while Path::new(&stat).exists() {
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
