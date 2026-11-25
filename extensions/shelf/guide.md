# File Shelf

File Shelf keeps owned copies of files you want at hand. The notch's Files tab
and the native drop shelf share the same copies, so a file stays put even if
the original moves or changes.

## Commands

- `vr shelf list` lists what is on the shelf.
- `vr shelf add <paths...>` copies files onto the shelf.
- `vr shelf remove <id>` removes one copy and `vr shelf clear` removes them
  all. Both ask for `--yes` first.

Every command accepts `--json`.
