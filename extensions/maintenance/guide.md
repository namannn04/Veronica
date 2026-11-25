# App Maintenance

App Maintenance shows what apt, snap and flatpak have installed, what can be
updated, and what removing a package would take with it, always before
anything changes. The disk cleaner measures developer caches and build output
and moves what you pick to the Trash.

## Commands

- `vr maintenance sources`, `inventory` and `updates` read only.
- `vr maintenance update <source> [names...]` prints the plan and changes
  nothing without `--yes`.
- `vr maintenance remove <package>` shows everything the removal takes with it
  and stops unless `--yes` is given. apt only.
- `vr cleaner categories`, `scan` and `drives` read only; `--root` also sweeps
  a folder for project output such as `node_modules`.
- `vr cleaner clean` re-scans and moves what it finds to the Trash with
  `--yes`.

Every command accepts `--json`.
