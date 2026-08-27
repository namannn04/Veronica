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

Requires Ubuntu 24.04 or later on x86_64.

See [docs/RUNNING.md](docs/RUNNING.md) for how to run it, use the notch, and
troubleshoot.

## Install

The Debian package is the recommended route on Ubuntu:

```
sudo apt install ./Veronica_0.1.8_amd64.deb
```

That installs the app, the `vr` command line tool, the desktop entry, the
AppStream metadata and the tray icon, and pulls in `jq`, which the usage
collector needs.

An AppImage is also published for other distributions:

```
chmod +x Veronica_0.1.8_amd64.AppImage
./Veronica_0.1.8_amd64.AppImage
```

Ubuntu 24.04 and later no longer ship the FUSE 2 runtime an AppImage needs to
mount itself. Either install it once with `sudo apt install libfuse2t64`, or run
the image without it:

```
./Veronica_0.1.8_amd64.AppImage --appimage-extract
./squashfs-root/AppRun
```

The AppImage does not install the `vr` command; use the Debian package for that.

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

**Machines**

- **One fleet view** — this computer plus any host you can already reach over
  SSH, each with live CPU, memory, disks, load and uptime from a single probe.
- **No new credentials** — remote machines are reached by running `ssh`, so your
  config aliases, keys, agent and jump hosts apply unchanged, and Veronica never
  handles a key or a passphrase.
- **Discovery** — aliases in `~/.ssh/config` are offered as one-click additions.
- **Honest failures** — an unreachable host says why, and never delays or hides
  the machines that answered.

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
vr media status              what is playing, on any MPRIS player
vr media toggle              play or pause; also next, previous, stop
vr media players             every player registered on the session bus
vr calendar list             your agenda, grouped by day, with join links
vr calendar next             the next event and how long until it starts
vr machines ls               the fleet, with live CPU, memory and disk
vr machines stats <id>       one machine's vital signs in full
vr machines add <ssh-host>   add a machine; discover finds config aliases
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
vr backup export             back up settings and persistent app data
vr backup inspect <file>     verify and describe an archive without restoring
vr backup import <file> --confirm  validate and restore an archive atomically
vr extensions                what can run on this session, and why not
vr config set <key> <value>  every setting the UI exposes
```

Every read command takes `--json`, stdout is exactly one document, logs go to
stderr, and exit codes are reliable, so an agent can drive Veronica headlessly.

## What works on your session

Veronica resolves each of its 25 capabilities against the running session and
reports the result, with the service it talks to, on the Diagnostics page and in
`vr diagnose`. Nothing is silently disabled.

Three capabilities need help on a Wayland session, because a Wayland compositor
deliberately withholds them from applications:

A note on the calendar: GNOME's calendar server expands recurring events but
does not pass an event's location or description through, so Veronica reads those
per event from Evolution Data Server to recover join links. That costs one D-Bus
call per event, which is why the notch's agenda skips it and the Calendar page
does not.

| Capability | Why | Route |
| --- | --- | --- |
| Keyboard lock | Needs an exclusive evdev grab | Membership of the `input` group |
| Local music playback | Not built yet; external players work through MPRIS | — |
| Companion | The backend's deployment is not shipped yet | — |

Clipboard history and Focus Dim were both in this table until the shell extension
existed. Capture and dimming now happen inside the compositor, which is the only
thing a Wayland session lets read the selection or place one window behind
another. Veronica reports a capability it has not built as needing integration
rather than as available, so the Extensions page never shows "Ready" for a switch
that would do nothing.

Everything else has a working route: PipeWire for audio and mic mute, logind for
prevent-sleep and lid-awake, MPRIS for media, Evolution Data Server for
calendar, and the desktop portal for colour picking, camera, global shortcuts and
screen-share detection.

## Privacy

Usage data never leaves this computer. Rate-limit checks are the one network
request Veronica makes: they go straight from this machine to your provider,
using the OAuth token your agent CLI already holds, and nothing is proxied
through anywhere else. There is no account and no telemetry.
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
