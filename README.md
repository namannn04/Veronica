# Veronica

A native desktop control center for Ubuntu. Veronica replaces a shelf of
single-purpose utilities with one application.

Free and open source under the [GPL-3.0](LICENSE). Every feature is in the one
app. No licence key, no account, no paid tier.

Veronica is a port of [Edith](https://github.com/pulkitxm/edith), a macOS
control center, to Linux. It keeps Edith's models — the capability map, the
extension catalogue, the rate-limit maths, the usage schema — and reimplements
the platform layer against Linux services. The usage collector is shared
verbatim, so the numbers are identical on both platforms.

Requires Ubuntu 24.04 or later on x86_64, with GNOME Shell 46 through 50.

See [docs/RUNNING.md](docs/RUNNING.md) for how to run it, use the notch, and
troubleshoot.

## Install

The Debian package is the recommended route on Ubuntu. This command downloads
the latest GitHub release, verifies its published SHA-256 checksum, and installs
it through `apt` (which may ask for your sudo password):

```bash
curl -fsSL https://raw.githubusercontent.com/namannn04/Veronica/main/install.sh | bash
```

To install a package you downloaded yourself instead:

```bash
sudo apt install ./Veronica_0.1.10_amd64.deb
```

That installs the app, the `vr` command line tool, the desktop entry, the
AppStream metadata and the tray icon, and pulls in `jq`, which the usage
collector needs, plus `curl` for the on-demand update check.

An AppImage is also published for other distributions:

```
chmod +x Veronica_0.1.10_amd64.AppImage
./Veronica_0.1.10_amd64.AppImage
```

Ubuntu 24.04 and later no longer ship the FUSE 2 runtime an AppImage needs to
mount itself. Either install it once with `sudo apt install libfuse2t64`, or run
the image without it:

```
./Veronica_0.1.10_amd64.AppImage --appimage-extract
./squashfs-root/AppRun
```

The AppImage is the desktop app only. It does not install the `vr` command or
the GNOME Shell extension, so the compact top-bar notch, clipboard capture,
Focus Dim, keystroke display and paste-in-place are unavailable. It also relies
on host tools such as `jq` and Bun or Node for usage collection. Use the Debian
package for Veronica's complete Ubuntu integration.

## Features

**Agent usage**

- **Local accounting** — Claude, Codex, Cursor and Command Code activity
  attributed to the right machine, repository, worktree and chat, without
  sending history anywhere.
- **Rate-limit rings** — Claude's session and weekly windows as live gauges,
  with second-by-second countdowns, and any model-scoped window the account has.
  Codex's windows too, read from its own server rather than the network.
- **Pace, not just percentage** — a window at 60% with minutes left matters more
  than one at 80% with a week to go, so each ring reports how far ahead of a
  linear burn you are and blends that into one risk reading.
- **Dashboard** — spend and token KPIs with per-day, model, source and hourly
  charts, plus a sortable model table.
- **Activity calendar** — a daily spend calendar across your whole history,
  ranked against your own busiest day.
- **Share cards** — export the highlights, the calendar, the daily rhythm and
  your busiest day as branded PNGs. A card never carries a repository name, a
  folder path, a chat title or a dollar cost: it exists to be posted, and those
  are the four fields that would leak a client, an employer or an income.
- **Project drilldown** — spend by project and repository, expanding into the
  chats that produced it.

**Alerts**

- **Every crossing, once** — a window rising past a level, falling back to green,
  drifting ahead of pace or burning hot each produce one banner when they happen,
  and silence while nothing changes. Edith's wording, verbatim.
- **Time-aware levels** — a session at 60% with minutes left outranks a week at
  80% with days to go, so the level follows the blended risk rather than the raw
  percentage.
- **Before a reset** — an optional reminder a chosen time before either window
  resets, fired by Veronica itself since freedesktop notifications cannot be
  scheduled.
- **One banner, updated** — a rising threshold replaces its own notification
  rather than stacking five, and the state survives a restart, so relaunching
  while a window sits at 90% does not re-announce it.
- **Nothing until asked** — off by default, and while off no request is made to
  any provider at all.

**Quinjet**

- **Your review workspaces** — the projects and worktrees Quinjet knows, on this
  computer or on any SSH machine, read through Quinjet's own JSON so the answer
  is the one the tool itself would give.
- **Opens where you work** — Quinjet's review TUI in the installed terminal, or
  `vr quinjet open` prints the exact command for a terminal you already have.
  Quinjet stays the owner of the review.

**Site Audit**

- **Edith's rules, exactly** — the same eleven checks and the same thresholds,
  so a page that passes on macOS passes here. Titles, descriptions, canonicals,
  headings, language, Open Graph, X cards and HTTPS.
- **Sitemap first** — `robots.txt`, then `/sitemap.xml`, then the page you gave
  it, following a sitemap index into its children.
- **A template problem, not a page problem** — the report counts each issue
  across the site, so one missing `og:image` and forty read differently.
- **Every run stays local** — Veronica fetches the pages being audited and
  nothing else. No result is uploaded and no third-party service is consulted.

**Herdr**

- **Live agent board** — every running Herdr agent grouped into Edith's Blocked,
  Working, Unknown, Done and Idle lanes, refreshed from Herdr's public JSON API.
- **Session and kind filters** — focus the board by persistent session or agent
  family, with a compact rail for agents and terminals.
- **Attach anywhere** — copy the exact attach command or open a session or agent
  directly in GNOME Console, while Herdr remains the owner of terminal state.

**This computer**

- **Live metrics** — CPU, memory, load, disks and thermal sensors from procfs
  and sysfs, with snap and loop mounts filtered out.
- **Microphone** — a system-wide mute switch through PipeWire.

**Calendar**

- **Your agenda** — every calendar configured on the machine, local or from an
  online account, with recurrences already expanded. Grouped by day, all-day
  events first, with what is happening now called out.
- **One-tap join** — meeting links recovered from the event's location,
  description or conference field, for Meet, Zoom, Teams, Jitsi, Webex and more.

**Media**

- **One place for every player** — anything that speaks MPRIS: Spotify, a browser
  tab, Rhythmbox, VLC. Now-playing with album art, transport control, and a
  progress bar that knows the difference between a track and a live stream.
- **Local library** — music under `~/Music` is indexed without uploading it,
  searchable by title, artist or album, and plays directly with queue, seeking,
  volume and hardware media-key support.

**Machines**

- **One fleet view** — this computer plus any host you can already reach over
  SSH, each with live CPU, memory, disks, load, uptime, temperatures, fan
  speeds, GPUs and its busiest processes, from a single probe. One round trip,
  no agent on the far end, nothing installed beyond a POSIX shell.
- **No new credentials** — remote machines are reached by running `ssh`, so your
  config aliases, keys, agent and jump hosts apply unchanged, and Veronica never
  handles a key or a passphrase.
- **Discovery** — aliases in `~/.ssh/config` are offered as one-click additions.
- **Files and terminals** — browse a local or remote home directory, download a
  file safely through SFTP, or open the host in the installed terminal.
- **Containers** — list Docker or Podman containers on any machine, read the
  last screenful of one's output, and start, stop or restart them without
  installing a Veronica agent on that host.
- **Power** — restart or shut down a remote machine, or wake one that is off
  with a Wake-on-LAN packet. Never on the computer Veronica is running on, and
  never escalated: the account it connects as has to be permitted already.
- **Honest failures** — an unreachable host says why, and never delays or hides
  the machines that answered.

**Global shortcuts**

- `Ctrl+Alt+V` opens Veronica, `Ctrl+Alt+B` opens Clipboard, `Ctrl+Alt+M`
  toggles the microphone, `Ctrl+Alt+P` picks a colour, `Ctrl+Alt+E` opens the
  emoji picker, and `Ctrl+Alt+K` enters keyboard-cleaning mode. GNOME Shell
  performs compositor-only actions.

**Clipboard**

- **Searchable history** — everything you copy, deduplicated so re-copying moves
  an entry up rather than adding a row, with a count of how often you have used
  it. One click copies it back.
- **Captured from the compositor** — on Wayland only the focused window may read
  the clipboard, so the GNOME Shell extension does the watching and the last few
  entries are available straight from the top bar dropdown.
- **Stays local** — a plain file under your data directory that you can inspect
  or delete; large copies are skipped rather than stored.

**Color Picker**

- **Sample any pixel** — the compositor's own eyedropper, through GNOME Shell or
  the desktop portal. Veronica never reads the screen without a click.
- **Every representation** — hex, `rgb()`, `rgba()`, `hsl()` and a `GdkRGBA`
  literal, from one pick. Choose which one lands on the clipboard.
- **Swatch history** — every colour you have sampled, kept locally, with the hex
  printed on the colour itself in whichever of black or white stays legible.
- **sRGB or Display P3** — the compositor reports sRGB; Display P3 converts it
  properly, for a wide-gamut panel.

**Emoji Picker**

- **Edith's catalogue, exactly** — the same emojibase release, the same names,
  the same search terms and the same ranking, so a search for "shrug" returns
  the same three characters in the same order on both platforms.
- **Skin tones and recents** — every tone variant, and a most-used list weighted
  by a 21-day half-life, so an old habit fades rather than sitting at the top
  forever.
- **Copy, or type it in place** — the clipboard always works; typing the emoji
  into the app you were in goes through the shell extension, because only the
  compositor may synthesise input into a window Veronica does not own.

**Keystroke Highlight**

- **A keycap for every press** — for demos and screen recordings, at the top or
  bottom of the screen, six at a time, each fading after a configurable 0.5 to
  3 seconds.
- **Listen-only, inside the compositor** — Wayland hands key presses only to the
  focused window, so the shell extension reads them and never consumes one:
  typing is unaffected whether the overlay is running or paused.
- **Nothing while a password is being typed** — while the shell holds a modal
  grab, the lock screen and authentication prompts included, nothing is
  recorded at all.

**Audio**

- **Per-application mixer** — every stream PipeWire holds, playing or recording,
  with its own volume and mute. A browser tab and a notification sound are two
  streams, and the mixer says so rather than pretending an app has one volume.
- **Bluetooth** — the adapters and devices BlueZ knows about, with each device's
  battery where it reports one. Read-only: Ubuntu's Quick Settings stays the
  only thing that pairs a device or switches the radio.

**Database**

- **Six engines** — PostgreSQL, MySQL, MariaDB, SQLite, Redis and Valkey.
  Browse the objects, read a bounded page, run a statement that reads.
- **Nothing unbounded** — every page has a hard ceiling and asks for one row
  more than it shows, so "is there more" is known rather than guessed. `KEYS *`
  is never sent; Redis is scanned.
- **A change is previewed, then confirmed, then applied** — the preview says
  what it would touch, what it cannot undo, and the exact text to type back. It
  hands you a token that is signed, expires, works once, and stops matching the
  moment anything about the plan changes. Edit the statement, a parameter, the
  table or the connection's policy after previewing and the apply is refused
  with nothing sent.
- **`query` never writes** — the check recognises the verbs that read and
  refuses everything else, so a statement type nobody thought of fails closed.
  Comments cannot hide a second statement.
- **Credentials in the keyring** — the Secret Service, the same place GNOME
  keeps everything else. A connection definition holds a reference, never a
  password, so it can be printed, logged and synced safely.
- **Read-only means read-only** — three separate switches refuse a change, and
  the refusal names which one. On PostgreSQL, MySQL and SQLite the server is
  told as well, so it is not only Veronica declining to send.
- **An MCP server for agents** — `vr database mcp` offers exactly two read-only
  tools. An agent learns which databases you have and what each can do; it never
  learns a host, a username or a credential.

**App Maintenance**

- **One update inventory** — apt, snap and flatpak in one list, with what is
  installed and what is available side by side. Checking installs nothing.
- **What you asked for, not its dependencies** — the apt inventory is the
  packages you chose, not the three thousand pulled in behind them.
- **Review-first removal** — apt's own simulation, so you see everything that
  would go with a package, and what would be left behind unused, before
  anything is removed.
- **Never escalates** — reads run unprivileged. A change that needs root goes
  through `pkexec`, so the desktop's own dialog asks you to authenticate;
  Veronica never wraps anything in `sudo` on your behalf.

**Disk cleaner**

- **Measures before it touches anything** — the developer caches sitting in your
  home directory, per category, with what removing each one actually costs you,
  and a sweep for `node_modules`, `target`, `.venv` and the rest inside any
  folder you point at.
- **Moves to the Trash, never deletes** — through the freedesktop trash spec, so
  Files can restore it. The Trash keeps occupying the disk until you empty it,
  which every screen that offers a clean says.
- **Your files only** — it walks your home directory with your own permissions,
  never asks for root, and never touches a system cache such as apt's.

**Presenter**

- **Blurs what should not be shared** — spend, token counts, rate limits,
  calendar entries and track names, while navigation and controls stay usable.
- **Notices a screen share** — an app capturing the screen or controlling it
  remotely activates it automatically, detected from the compositor's own
  screencast sessions. Dismiss one share without switching detection off.
- **Category by category** — reveal the calendar for a demo while spend stays
  hidden.

**Focus Dim**

- **Darkens everything behind your window** — drawn inside GNOME Shell, so it
  works on Wayland, where no application could place a dim behind another app's
  window.
- **Per display or focused only** — keep each monitor's front window bright, or
  dim every monitor but the one you are typing in.

**Appearance**

- **Thirteen themes, one setting** — System follows Ubuntu's own light/dark
  choice. Four light: Ivory, Sandstone, Mist and Paper. Eight dark: Graphite,
  Midnight, Aubergine, Forest, Nord, Ocean, Ember and Carbon.
- **The app and the notch together** — one `appearance` setting themes the
  desktop window and the GNOME shelf, so the two never disagree, and anything
  Veronica launches with a theme of its own gets the light-or-dark answer
  rather than the theme's name.
- **Charts follow the family** — every theme declares whether it is light or
  dark, and the categorical and sequential chart ramps are chosen from that,
  so a chart is readable on Carbon's true black and on Paper's white alike.
- **Paper and Carbon are the accessibility pair** — Paper trades the soft
  hairlines for borders that clear the 3:1 non-text minimum on their own;
  Carbon paints an unlit `#000` page for OLED panels.

**Getting around**

- **Ctrl+K** — search every page by name, by what it does, or by a word you
  would reach for: "ssh" finds Machines, "apt" finds App Maintenance, "theme"
  finds Settings.
- **A grouped rail** — nineteen pages sorted into Agents, This computer, Tools
  and Veronica, rather than one column to read top to bottom.

**Top bar**

- **CPU and memory beside the clock** — Edith's menu bar readout, in the position
  Ubuntu has for it, read straight from procfs rather than by running anything.
- **Compact Edith shelf** — while the extension is enabled, Veronica replaces
  only the center date button with its 580px Home, Notifications, Files,
  Clipboard and Camera shelf. It does not lengthen GNOME's calendar dropdown.
- **Ubuntu controls stay native** — Quick Settings remains the only owner of
  Wi-Fi, Bluetooth, volume and battery. Veronica adds neither copies of those
  indicators nor an agent-spend indicator to the right side of the panel.
- **No second panel** — the shell keeps owning the top bar; Veronica only adds
  what the shell has no idea about.

**Media**

- **One place for every player** — anything that speaks MPRIS: Spotify, a browser
  tab, Rhythmbox, VLC. Now-playing with album art, transport control, and a
  progress bar that knows the difference between a track and a live stream.

**Notch**

- **The clock dropdown, upgraded** — click Ubuntu's center clock as usual. One
  panel holds now-playing with album art and transport, the notification
  history, a month grid, the day's events, and the things the shell knows
  nothing about: agent spend and machine load.
- **Edith's structure** — Home, Files, Clipboard, Audio and Camera use the same
  pill-tab order as Edith. Home has the paired now-playing and limit cards plus
  real Keep Awake and Lid Awake actions; Clipboard is a scrollable copy/delete
  history. Tabs whose Linux backend is still being ported identify that state
  instead of presenting a dead control.

## Command line

Installing Veronica installs `vr`, which reaches the same operations as the UI.

```
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
vr herdr board               the live agent board, in Edith's lanes
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
vr extensions                what can run on this session, and why not
vr tools                     the command line programs the extensions need
vr tools install <tool>      fetch one, when there is a route Veronica can drive
vr config set <key> <value>  every setting the UI exposes
```

Every read command takes `--json`, stdout is exactly one document, logs go to
stderr, and exit codes are reliable, so an agent can drive Veronica headlessly.

## What works on your session

Veronica resolves each of its 31 capabilities against the running session and
reports the result, with the service it talks to, on the Diagnostics page and in
`vr diagnose`. Nothing is silently disabled.

Compositor-owned features ask for permission or the GNOME extension when the
session requires it. Local music, keyboard cleaning and Companion all have
native Linux implementations and no longer need external integration.

A note on the calendar: GNOME's calendar server expands recurring events but
does not pass an event's location or description through, so Veronica reads those
per event from Evolution Data Server to recover join links. That costs one D-Bus
call per event, which is why the notch's agenda skips it and the Calendar page
does not.

Clipboard history and Focus Dim were restricted until the shell extension
existed. Capture and dimming now happen inside the compositor, which is the only
thing a Wayland session lets read the selection or place one window behind
another. Veronica reports a capability it has not built as needing integration
rather than as available, so the Extensions page never shows "Ready" for a switch
that would do nothing.

**Companion** is local-first: notes and searchable voice-memo metadata are stored
under the XDG data directory, recordings use PipeWire, and the complete folder is
included in Veronica backup/restore. It needs no Docker service or account.

Keystroke Highlight, the emoji picker's insert-in-place and paste-in-place all
run through the shell extension for the same reason clipboard capture and
dimming do: on Wayland only the compositor may observe or synthesise input for a
window it does not own. Each says so on the Diagnostics page rather than
appearing to work and doing nothing.

Everything else has a working route: PipeWire for audio, the per-application
mixer and mic mute, BlueZ for Bluetooth, logind for prevent-sleep and lid-awake,
MPRIS for media, Evolution Data Server for calendar, and the desktop portal for
colour picking, camera, global shortcuts and screen-share detection.

## Privacy

Usage history never leaves this computer. Provider rate-limit checks go straight
from this machine to the provider, using the OAuth token your agent CLI already
holds, and nothing is proxied through Veronica. There is no Veronica account and
no telemetry. Network access otherwise happens only for features that inherently
need it or that you explicitly run: checking GitHub for an update, auditing a
website, connecting to a remote machine or database, and downloading a tool you
asked Veronica to install. Background provider checks run only when their alert
switches are enabled.
Veronica reads your local agent history from `~/.claude`, `~/.codex`,
`~/.cursor` and `~/.commandcode`, and writes its own state under the XDG
directories, which `vr diagnose` prints.

## Building from source

Needs Rust, Bun and the Tauri system dependencies:

```
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
  libayatana-appindicator3-dev libjavascriptcoregtk-4.1-dev \
  libsoup-3.0-dev libxdo-dev patchelf

cargo test --workspace
cd apps/desktop && bun install && bunx tauri build --bundles deb
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the crate layout, the
capability model, and why this is a reimplementation rather than a Swift port.

## Licence

Veronica is free software licensed under the GNU General Public License v3.0,
matching Edith, from which its models are derived.
