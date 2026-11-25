# Developer Workspace

Developer Workspace inspects a project without running its code and gives you
a playground for a local Graftcode Gateway.

## Commands

- `vr workspace doctor --root <path>` checks a project: toolchain files,
  expected listeners (`--ports 3000,5432`) and expected Docker containers
  (`--containers db,cache`). It never executes the project or reads `.env`
  files.
- `vr workspace graft discover --endpoint <url>` lists what a local Graftcode
  Gateway offers.
- `vr workspace graft call --endpoint <url> --name <method> --arguments '{}'`
  calls one method. Methods that change something need `--confirm`.

Set `VERONICA_GRAFT_TOKEN` when the gateway expects a token. Every command
accepts `--json`.
