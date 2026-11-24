use std::{io::Read, os::unix::fs::MetadataExt};
pub(crate) fn stop(key: &str, owner: &str) {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return;
    };
    let expected = format!("{key}={owner}");
    let uid = unsafe { libc::geteuid() };
    for entry in entries.take(8192).flatten() {
        if entry.metadata().map_or(true, |m| m.uid() != uid) {
            continue;
        }
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<i32>() else {
            continue;
        };
        let Ok(fd) = i32::try_from(unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) }) else {
            continue;
        };
        if fd < 0 {
            continue;
        }
        let matches = (|| {
            let file = std::fs::File::open(entry.path().join("environ")).ok()?;
            let mut bytes = Vec::new();
            file.take(128 * 1024).read_to_end(&mut bytes).ok()?;
            Some(bytes.split(|b| *b == 0).any(|v| v == expected.as_bytes()))
        })();
        unsafe {
            if matches == Some(true) {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    fd,
                    libc::SIGKILL,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                );
            }
            libc::close(fd);
        }
    }
}
