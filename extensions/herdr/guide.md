# Herdr

Herdr keeps coding-agent sessions alive in persistent terminal workspaces.
This extension shows its board, opens a session in your terminal, or attaches
to one agent inside Veronica.

## Commands

- `vr herdr board` lists every agent grouped into lanes by state
  (`vr herdr ls` is the same). `--session`, `--kind` and `--status` narrow it.
- `vr herdr sessions` lists the persistent sessions.
- `vr herdr attach <session> [pane]` prints the command that attaches to a
  session or one agent, so you can run it in any terminal, even over SSH.

Every command accepts `--json`.

## Watching in Veronica

Attaching from the page takes over the agent's existing Herdr frontend and
streams its terminal into Veronica. Herdr itself must be installed on this
computer.
