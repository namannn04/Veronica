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

## Install

The Debian package is the recommended route on Ubuntu:

```
sudo apt install ./Veronica_0.1.0_amd64.deb
```

That installs the app, the `vr` command line tool, the desktop entry, the
AppStream metadata and the tray icon, and pulls in `jq`, which the usage
collector needs.

An AppImage is also published for other distributions:

```
chmod +x Veronica_0.1.0_amd64.AppImage
./Veronica_0.1.0_amd64.AppImage
```

Ubuntu 24.04 and later no longer ship the FUSE 2 runtime an AppImage needs to
mount itself. Either install it once with `sudo apt install libfuse2t64`, or run
the image without it:

```
./Veronica_0.1.0_amd64.AppImage --appimage-extract
./squashfs-root/AppRun
```

The AppImage does not install the `vr` command; use the Debian package for that.

## Features

**Agent usage**

- **Local accounting** — Claude, Codex, Cursor and Command Code activity
  attributed to the right machine, repository, worktree and chat, without
  sending history anywhere.
- **Dashboard** — spend and token KPIs with per-day, model, source and hourly
  charts, plus a sortable model table.
- **Activity calendar** — a daily spend calendar across your whole history,
  ranked against your own busiest day.
- **Project drilldown** — spend by project and repository, expanding into the
  chats that produced it.

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

**Notch**

- **Hover island** — a pill tucked under the top bar showing the clock, today's
  spend, the current track and live indicators, expanding on hover into
  now-playing with transport, agent usage, machine load, a file shelf and quick
  actions. Clicks pass through everywhere the island is not.

## Command line

Installing Veronica installs `vr`, which reaches the same operations as the UI.

```
vr diagnose                  the resolved session, capabilities and extensions
vr usage summary             the same totals the dashboard shows
vr usage projects --chats    spend by project, with its chats
vr usage calendar --days 30  the spend calendar in the terminal
vr usage refresh --progress  run the collector
vr media status              what is playing, on any MPRIS player
vr media toggle              play or pause; also next, previous, stop
vr media players             every player registered on the session bus
vr calendar list             your agenda, grouped by day, with join links
vr calendar next             the next event and how long until it starts
vr extensions                what can run on this session, and why not
vr config set <key> <value>  every setting the UI exposes
```

Every read command takes `--json`, stdout is exactly one document, logs go to
stderr, and exit codes are reliable, so an agent can drive Veronica headlessly.

## What works on your session

Veronica resolves each of its 24 capabilities against the running session and
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
| Clipboard history | Only the focused window may read the clipboard | GNOME Shell extension, or an X11 session |
| Focus Dim | Dimming other windows is the compositor's job | GNOME Shell extension, or an X11 session |
| Keyboard lock | Needs an exclusive evdev grab | Membership of the `input` group |

Everything else has a working route: PipeWire for audio and mic mute, logind for
prevent-sleep and lid-awake, MPRIS for media, Evolution Data Server for
calendar, and the desktop portal for colour picking, camera, global shortcuts and
screen-share detection.

## Privacy

Usage data never leaves this computer. There is no account and no telemetry.
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
