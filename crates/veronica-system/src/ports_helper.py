"""Host-side procfs access, including from Flatpak. Standard-library Python only."""
import ipaddress
import json
import os
from pathlib import Path
import signal
import sys


def parse_table(text, protocol, ipv6=False):
    rows = []
    for line in text.splitlines()[1:]:
        fields = line.split()
        if len(fields) < 10 or fields[3] != ("0A" if protocol == "TCP" else "07"):
            continue
        try:
            raw, port = fields[1].split(":")
            # procfs writes each 32-bit word in native byte order.
            packed = b"".join(int(raw[i:i + 8], 16).to_bytes(4, sys.byteorder)
                              for i in range(0, len(raw), 8))
            if len(packed) != (16 if ipv6 else 4):
                continue
            rows.append({"address": str(ipaddress.ip_address(packed)),
                         "port": int(port, 16), "protocol": protocol,
                         "inode": fields[9], "owners": []})
        except (ValueError, OverflowError):
            continue
    return rows


def identity(pid):
    root = Path("/proc") / str(pid)
    # A process name may itself contain spaces and closing parentheses.
    stat = (root / "stat").read_text().rsplit(")", 1)[1].split()
    uid = next(line.split()[1] for line in (root / "status").read_text().splitlines()
               if line.startswith("Uid:"))
    return {"pid": pid, "startedAt": stat[19], "uid": int(uid),
            "name": (root / "comm").read_text().strip()}


def sockets(pid):
    found = set()
    for fd in (Path("/proc") / str(pid) / "fd").iterdir():
        try:
            link = os.readlink(fd)
            if link.startswith("socket:[") and link.endswith("]"):
                found.add(link[8:-1])
        except OSError:
            pass
    return found


def snapshot():
    rows, warnings = [], []
    for name, protocol, ipv6 in [("tcp", "TCP", False), ("tcp6", "TCP", True),
                                 ("udp", "UDP", False), ("udp6", "UDP", True)]:
        try:
            rows.extend(parse_table((Path("/proc/net") / name).read_text(), protocol, ipv6))
        except FileNotFoundError:
            if not ipv6:
                warnings.append(f"{protocol} socket information is unavailable")
        except OSError as error:
            warnings.append(f"Cannot read {name}: {error.strerror}")
    indexed = {}
    for row in rows:
        indexed.setdefault(row["inode"], []).append(row)
    restricted = False
    for root in Path("/proc").iterdir():
        if not root.name.isdecimal():
            continue
        pid = int(root.name)
        try:
            owner = identity(pid)
            owned = sockets(pid)
            # Reject a process which exited/reused its PID during collection.
            if identity(pid)["startedAt"] != owner["startedAt"]:
                continue
            owner["canStop"] = (owner["uid"] == os.getuid() == os.geteuid()
                                and pid > 1 and pid != os.getpid()
                                and hasattr(os, "pidfd_open")
                                and hasattr(signal, "pidfd_send_signal"))
            for inode in owned.intersection(indexed):
                for row in indexed[inode]:
                    row["owners"].append(owner)
        except PermissionError:
            restricted = True
        except (OSError, ValueError, IndexError, StopIteration):
            pass
    if restricted or any(not row["owners"] for row in rows):
        warnings.append("Linux permissions may hide some process owners. Hidden or other-user processes cannot be stopped here.")
    for row in rows:
        row["owners"].sort(key=lambda owner: owner["pid"])
    return {"ports": sorted(rows, key=lambda row: (row["port"], row["protocol"], row["address"], row["inode"])),
            "warnings": warnings}


def stop(request):
    pid = request["pid"]
    if not isinstance(pid, int) or pid <= 1 or pid == os.getpid():
        raise ValueError("This process cannot be stopped")
    if not hasattr(os, "pidfd_open") or not hasattr(signal, "pidfd_send_signal"):
        raise ValueError("Safe process stopping requires Linux pidfd support")
    # Hold a kernel reference before revalidating; never signal a reused PID.
    fd = os.pidfd_open(pid)
    try:
        owner = identity(pid)
        if owner["uid"] != os.getuid() or os.getuid() != os.geteuid():
            raise PermissionError("Only your own processes can be stopped")
        if owner["startedAt"] != request["startedAt"]:
            raise ValueError("The process changed. Refresh before trying again")
        if request["inode"] not in sockets(pid):
            raise ValueError("This process no longer owns that port. Refresh first")
        signal.pidfd_send_signal(fd, signal.SIGTERM)
    finally:
        os.close(fd)
    return {"message": "Stop requested. A supervisor may restart the process; refresh to check."}


def main():
    try:
        request = json.loads(sys.argv[1])
        result = stop(request) if request.get("action") == "stop" else snapshot()
        print(json.dumps(result))
    except (OSError, ValueError, KeyError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
