# Machines

Machines lists this computer and the hosts you reach over SSH, with their
vital signs, files, containers and a terminal one click away. Nothing is
installed on the other side: every probe is a short script sent over your own
SSH configuration.

## Commands

- `vr machines list` probes every machine; `--offline` only shows what is
  configured.
- `vr machines add <target>` adds a host by alias or `user@host`, with
  `--name`, `--port` and `--mac` for Wake-on-LAN. `discover` lists aliases from
  `~/.ssh/config` that are not added yet.
- `vr machines stats [id]`, `containers [id]` and `logs <id> <container>` read
  only.
- `vr machines container <id> <container> <action>`, `power <id> <action>` and
  `remove <id>` change things and ask for `--yes` first. `wake <id>` sends a
  Wake-on-LAN packet.

Every command accepts `--json`.
