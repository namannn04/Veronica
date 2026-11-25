# Color Picker

Color Picker opens the screen colour sampler, records what you pick in a
swatch history and copies it in the format you chose. The history stays on
this computer.

## Commands

- `vr color pick` samples a colour, records it and copies it. `--format`
  picks the representation for this run and `--no-copy` only records it.
- `vr color list` prints the history, newest first (`--limit`, 20 by default).
  `vr color ls` is the same command.
- `vr color copy <id>` copies a swatch again, optionally in another
  `--format`.
- `vr color show <id>` prints one swatch in every format.
- `vr color remove <id>` forgets one swatch and `vr color clear` forgets them
  all.

Every command accepts `--json`.

## Shortcut

Ctrl+Alt+P runs `vr color pick` from anywhere. Change it under Settings,
Shortcuts.
