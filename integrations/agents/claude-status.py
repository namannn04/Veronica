#!/usr/bin/env python3
"""Opt-in Claude status only: never persist prompts, tool arguments or transcripts."""

import contextlib
import itertools
import json
import os
import pathlib
import re
import sys
import time

STALE_SECONDS = 86400


def main() -> None:
    data = json.loads(sys.stdin.read(32769))
    status = {
        "SessionStart": "idle",
        "UserPromptSubmit": "working",
        "PreToolUse": "working",
        "PostToolUse": "working",
        "PermissionRequest": "blocked",
        "Stop": "done",
        "SessionEnd": "idle",
    }.get(data.get("hook_event_name"))
    if not status:
        return
    pid = os.getppid()
    for _ in range(12):
        path = pathlib.Path("/proc") / str(pid)
        comm = (path / "comm").read_text().strip()
        fields = (path / "stat").read_text().rsplit(") ", 1)[1].split()
        if comm == "claude":
            break
        pid = int(fields[1])
        if pid <= 1:
            return
    else:
        return
    root = pathlib.Path(__file__).parent / "events"
    if root.is_symlink():
        return
    root.mkdir(mode=0o700, exist_ok=True)
    target = root / (str(pid) + "-" + fields[19] + ".json")
    temporary = root / ("." + str(os.getpid()) + ".tmp")
    fd = os.open(temporary, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    with os.fdopen(fd, "w") as out:
        json.dump({"status": status, "updated": int(time.time())}, out)
    temporary.replace(target)
    for item in itertools.islice(root.iterdir(), 256):
        if (
            re.fullmatch(r"\d+-\d+\.json", item.name)
            and item != target
            and not item.is_symlink()
            and item.is_file()
            and time.time() - item.stat().st_mtime > STALE_SECONDS
        ):
            item.unlink()


with contextlib.suppress(Exception):
    main()
