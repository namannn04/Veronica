# System

System shows what this computer is doing: CPU, memory, disks, temperatures
and battery, the running apps, the audio mixer and Bluetooth devices. A second
page lists listening ports and local Docker containers. Home gets live CPU and
memory cards.

## Commands

- `vr system snapshot`, `stats`, `disks` and `processes` read the machine.
- `vr system audio list|volume|mute|unmute` drive the per-application mixer;
  `vr system bluetooth` lists adapters and devices.
- `vr ports list` shows TCP and UDP listeners; `vr ports stop` sends SIGTERM
  after checking the process and socket identity, and asks for `--yes` first.
- `vr docker status` reads the local daemon; `vr docker action` starts, stops
  or restarts a container and asks for `--yes` first.

Every command accepts `--json`.
