# Attention

Attention keeps focus sessions and, with Veronica's GNOME Shell extension,
a local history of which application had your attention. Nothing leaves this
computer.

## Commands

- `vr attention status` shows the active timer and your completed focus total.
- `vr attention focus start --for 45m` starts a timer (`--name` labels it),
  `vr attention focus stop` finishes it and `vr attention focus status` shows
  it.
- `vr attention history` lists completed sessions, newest first.
- `vr attention summary --days 7` summarises application activity.
- `vr attention configure --enabled true --privacy applications` turns
  tracking on or off and picks how much it records. `--idle-seconds` sets
  when you count as away.

Every command accepts `--json`.

## Tracking

The shell extension records one pulse every ten seconds while tracking is on.
With `applications` privacy only the application is stored; `detailed` also
keeps window titles.
