# Emoji

Emoji searches the full emoji catalogue, copies what you pick and remembers
what you reach for most. Ctrl+Alt+E opens the compact picker over whatever
you are typing in; change the shortcut under Settings, Shortcuts.

## Commands

- `vr emoji list [query]` searches the catalogue (`vr emoji search` is the
  same command). `--group` keeps one group, `--tone` picks a skin tone and
  `--limit` caps the results (20 by default).
- `vr emoji groups` lists the groups.
- `vr emoji copy <query>` copies the best match and records the pick.
  `--no-copy` only records it.
- `vr emoji recents` lists the emoji you use most.
- `vr emoji forget <emoji>` forgets one emoji's usage, and
  `vr emoji forget --all` forgets all of it.

Every command accepts `--json`.
