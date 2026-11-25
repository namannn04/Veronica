# Calendar

Calendar reads the events GNOME already knows about, through the GNOME Shell
calendar server and Evolution Data Server. Add accounts in Settings, Online
Accounts, and their events show up here, on Home and in the notch.

## Commands

- `vr calendar list` prints the agenda grouped by day. `--days` sets how far
  ahead to look (7 by default) and `--all` keeps events that already finished
  today. `vr calendar agenda` is the same command.
- `vr calendar next` prints the next event that has not started yet.

Every command accepts `--json`.

## Home and the notch

While the app runs, Calendar keeps a short list of what is coming up on Home
and in the notch. Presenter mode blurs it when calendar entries are hidden.
