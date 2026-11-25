# Keystroke Highlight

Keystroke Highlight shows a keycap for every key press, for demos and
recordings. Veronica's GNOME Shell extension reads and draws them, never
consumes a press, and records nothing while the lock screen or a password
prompt is up.

## Commands

- `vr keystroke-highlight status` reports the settings and whether this
  desktop can apply them. `vr keys` is a shorter name for the same commands.
- `vr keystroke-highlight on` starts the overlay and `off` pauses it.
- `vr keystroke-highlight enable` and `disable` turn the feature on and off.
- `vr keystroke-highlight duration 1.5` sets how long a keycap stays.
- `vr keystroke-highlight position top` moves the row to the top or bottom.

Every command accepts `--json`.
