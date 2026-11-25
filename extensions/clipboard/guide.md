# Clipboard

Clipboard keeps a searchable history of what you copy. The GNOME Shell
extension records each copy while this extension is on, skipping anything that
looks like a password or a one-time code, and the page copies any entry back
with one click.

## Commands

- `vr clipboard list` shows the newest entries; `--query` filters them and
  `--limit` caps the list.
- `vr clipboard get <id>` prints one entry's full text, ready to pipe onward.
- `vr clipboard record` stores text read from standard input. The shell uses
  it; re-copying something moves it to the front instead of duplicating it.
- `vr clipboard remove <id>` and `vr clipboard clear` forget entries. Both ask
  for `--yes` first.

Every command accepts `--json`.
