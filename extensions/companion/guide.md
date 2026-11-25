# Companion

Companion keeps your notes and voice memos in one searchable place on this
computer. Write a note, record a memo with PipeWire, pin what matters and play
memos back from the page.

## Commands

- `vr companion list [query]` lists everything, newest first; `--kind note`
  or `--kind voice` narrows it.
- `vr companion show <id>` prints one item in full.
- `vr companion add <title> [body]` writes a note. Without a body it reads
  stdin, so a file can be piped in. `--pinned` pins it straight away.
- `vr companion pin <id>` pins an item; `--off` unpins it.
- `vr companion remove <id>` deletes an item and its recording, and asks for
  `--yes` first.

Every command accepts `--json`.
