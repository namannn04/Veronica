import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import time
import unittest

SCRIPT = pathlib.Path(__file__).with_name("claude-status.py")


class StatusHookTests(unittest.TestCase):
    def run_hook(self, root, event):
        program = """import ctypes,json,subprocess,sys
ctypes.CDLL(None).prctl(15,b'claude',0,0,0)
subprocess.run([sys.executable,sys.argv[1]],input=json.dumps({'hook_event_name':sys.argv[2],'prompt':'PRIVATE_PROMPT','tool_input':{'secret':'PRIVATE_KEY'}}),text=True,check=True)
"""
        subprocess.run(
            ["python3", "-c", program, str(root / "status.py"), event], check=True, timeout=5
        )

    def test_only_status_is_stored_and_unknown_files_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            shutil.copy(SCRIPT, root / "status.py")
            events = root / "events"
            events.mkdir()
            keep = events / "keep.txt"
            keep.write_text("unrelated")
            os.utime(keep, (time.time() - 200000, time.time() - 200000))
            self.run_hook(root, "PermissionRequest")
            files = list(events.glob("*.json"))
            self.assertEqual(len(files), 1)
            value = json.loads(files[0].read_text())
            self.assertEqual(set(value), {"status", "updated"})
            self.assertEqual(value["status"], "blocked")
            self.assertNotIn("PRIVATE", files[0].read_text())
            self.assertEqual(keep.read_text(), "unrelated")

    def test_symlink_events_directory_is_never_followed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            shutil.copy(SCRIPT, root / "status.py")
            victim = root / "other"
            victim.mkdir()
            (root / "events").symlink_to(victim, target_is_directory=True)
            self.run_hook(root, "Stop")
            self.assertEqual(list(victim.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
