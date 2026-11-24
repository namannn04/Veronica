"""Owned Ollama supervisor: parent pipe EOF also cleans up after an abrupt app exit."""

import contextlib
import os
import pathlib
import signal
import subprocess
import sys

child = None
owner = os.environ.get("VERONICA_OWNED_LOCAL_RUNTIME", "")
try:
    child = subprocess.Popen(
        [sys.argv[1], "serve"],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
    )
    sys.stdin.buffer.read()
finally:
    if child and child.poll() is None:
        with contextlib.suppress(ProcessLookupError):
            os.killpg(child.pid, signal.SIGKILL)
    if owner:
        import itertools

        expected = ("VERONICA_OWNED_LOCAL_RUNTIME=" + owner).encode()
        for entry in itertools.islice(pathlib.Path("/proc").iterdir(), 8192):
            try:
                pid = int(entry.name)
                if pid == os.getpid() or entry.stat().st_uid != os.getuid():
                    continue
                fd = os.pidfd_open(pid)
                try:
                    with (entry / "environ").open("rb") as stream:
                        env = stream.read(131072)
                    if expected in env.split(b"\0"):
                        signal.pidfd_send_signal(fd, signal.SIGKILL)
                finally:
                    os.close(fd)
            except (ValueError, PermissionError, FileNotFoundError, ProcessLookupError):
                pass
    if child:
        try:
            child.wait(timeout=2)
        except subprocess.TimeoutExpired:
            child.kill()
