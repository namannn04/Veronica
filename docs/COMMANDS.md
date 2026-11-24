# Command line

Installing Veronica installs `vr`, which reaches the same operations as the app.
Every read command takes `--json`, stdout is exactly one document, logs go to
stderr, and exit codes are reliable.

```text
vr diagnose                  the resolved session, capabilities and extensions
vr usage summary             the same totals the dashboard shows
vr usage projects --chats    spend by project, with its chats
vr usage calendar --days 30  the spend calendar in the terminal
vr usage refresh --progress  run the collector
vr usage limits              rate-limit gauges, with pace and countdowns
vr usage export --card activity   branded PNG cards, safe to post
vr media status              what is playing, on any MPRIS player
vr media toggle              play or pause; also next, previous, stop
vr media players             every player registered on the session bus
vr calendar list             your agenda, grouped by day, with join links
vr calendar next             the next event and how long until it starts
vr machines ls               the fleet, with live CPU, memory and disk
vr machines stats <id>       one machine's vital signs in full
vr machines add <ssh-host>   add a machine; discover finds config aliases
vr machines containers <id>  Docker or Podman containers; logs tails one
vr machines power <id> restart --confirm   restart or shut down a machine
vr machines wake <id>        a Wake-on-LAN packet, for a machine that is off
vr clipboard list            the clipboard history, searchable
vr clipboard get <id>        one entry's full text, for piping onward
vr usage alerts              what the notifier would post right now, as a dry run
vr alerts test               post one banner, to check notifications arrive
vr alerts state              what the notifier is comparing against
vr color pick                sample a pixel; copies and records it
vr color list                the swatch history
vr color show <id>           one swatch in every format
vr presenter status          the blur state, and what detection sees
vr presenter start           blur now; also stop, enable, disable, dismiss
vr focus-dim on              dim behind the focused window; also off, intensity
vr keystroke-highlight on    show key presses on screen; also off, duration
vr emoji list shrug          search the catalogue; copy puts one on the clipboard
vr emoji recents             the emoji you reach for most
vr system audio list         the per-application mixer, with volumes
vr system bluetooth          adapters and devices, with their batteries
vr system ps                 running processes, by CPU or memory
vr power lid-awake on --for 30m  keep running with the lid shut for 30 minutes
vr herdr board               the live agent board, grouped by state
vr quinjet projects          Quinjet's review workspaces; open prints the command
vr audit site example.com    crawl a sitemap and check every page's metadata
vr companion list            your notes and voice memos, searchable
vr media library             the local music library under ~/Music
vr maintenance updates       what apt, snap and flatpak could update
vr maintenance remove <pkg>  what removing it would take with it
vr database connections      your saved databases; add takes a password on stdin
vr database query <c> "…"    run a statement that reads; a write is refused
vr database mutations preview   what a change would do, and a one-shot token
vr database mcp              two read-only tools for an agent, over stdio
vr cleaner scan              what could be reclaimed, per category
vr cleaner clean --yes       move it to the Trash; --root sweeps a project tree
vr backup export             back up settings and persistent app data
vr backup inspect <file>     verify and describe an archive without restoring
vr backup import <file> --confirm  validate and restore an archive atomically
vr extensions                every extension, where it came from, and what it can run
vr extensions install <id>   preview an install; --yes downloads, verifies and installs
vr extensions doctor         exec permission, backends, tools and the catalog
vr tools                     the command line programs the extensions need
vr tools install <tool>      fetch one, when there is a route Veronica can drive
vr config set <key> <value>  every setting the UI exposes
```
