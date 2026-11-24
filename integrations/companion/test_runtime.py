import os
import pathlib
import subprocess
import tempfile
import time
import unittest
import uuid

SCRIPT = pathlib.Path(__file__).with_name("local-runtime.py")


class RuntimeTests(unittest.TestCase):
    def test_pipe_eof_stops_owned_runtime(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            fake = root / "fake-ollama"
            pidfile = root / "child.pid"
            fake.write_text(
                '#!/usr/bin/env python3\nimport os,time,pathlib\npathlib.Path(os.environ["TEST_RUNTIME_PIDFILE"]).write_text(str(os.getpid()))\ntime.sleep(60)\n'
            )
            fake.chmod(0o700)
            env = dict(
                os.environ,
                VERONICA_OWNED_LOCAL_RUNTIME=str(uuid.uuid4()),
                TEST_RUNTIME_PIDFILE=str(pidfile),
            )
            child = subprocess.Popen(
                ["python3", str(SCRIPT), str(fake)], env=env, stdin=subprocess.PIPE
            )
            try:
                for _ in range(100):
                    if pidfile.exists():
                        break
                    time.sleep(0.02)
                self.assertTrue(pidfile.exists())
                pid = int(pidfile.read_text())
                child.stdin.close()
                child.wait(timeout=5)
                self.assertFalse(pathlib.Path("/proc", str(pid)).exists())
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait()


if __name__ == "__main__":
    unittest.main()
