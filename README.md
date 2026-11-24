<h1 align="center">
  <img src="branding/veronica-logo.png" width="96" height="96" alt="Veronica logo"><br>
  Veronica
</h1>

<p align="center">
  <a href="https://github.com/namannn04/Veronica/actions/workflows/ci.yml"><img src="https://github.com/namannn04/Veronica/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/namannn04/Veronica/releases/latest"><img src="https://img.shields.io/github/v/release/namannn04/Veronica" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0%20%2B%20attribution-blue.svg" alt="License: GPL-3.0 with attribution"></a>
</p>

A native desktop control center for Ubuntu. Agent usage, system tools, media,
clipboard and GNOME integration in one app, with no account, no telemetry and
no paid tier.

Requires Ubuntu 24.04 or later on x86_64 with GNOME Shell 46 to 50. Screenshots
and shortcuts are on the [website](https://veronica.namandadhich.in/).

## Install

```bash
curl -fsSL https://veronica.namandadhich.in/install | bash
```

This verifies and installs the latest Debian package and turns on the GNOME
Shell extension. Log out and back in once to see the notch.

Other ways to install:

```bash
flatpak install --user https://veronica.namandadhich.in/flatpak/veronica.flatpakref
```

To get updates through `apt upgrade` from the signed apt repository instead:

```bash
curl -fsSL https://veronica.namandadhich.in/apt/veronica.gpg | sudo tee /etc/apt/keyrings/veronica.gpg >/dev/null
echo "deb [signed-by=/etc/apt/keyrings/veronica.gpg] https://veronica.namandadhich.in/apt stable main" | sudo tee /etc/apt/sources.list.d/veronica.list
sudo apt update && sudo apt install veronica
```

An AppImage, `Veronica_0.1.26_amd64.AppImage`, is on the
[latest release](https://github.com/namannn04/Veronica/releases/latest). It is
the desktop app only, without the `vr` command or the shell extension.

Releases also attach a classic snap, `Veronica_<version>_amd64.snap`. Until it
is listed in the Snap Store, install the downloaded file directly; it installs
`vr` as `veronica.vr`:

```bash
sudo snap install --classic --dangerous ./Veronica_<version>_amd64.snap
```

## Features

| Area               | What you get                                                                                                                                                   |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Agent usage        | Local spend and token accounting for Claude, Codex, Cursor and Command Code, rate-limit rings with pace, a dashboard, an activity calendar and shareable cards |
| Alerts             | One notification per threshold crossing, time-aware levels and optional reset reminders, off by default                                                        |
| This computer      | CPU, memory, disks, sensors, processes, a per-app audio mixer, Bluetooth and a mic mute switch                                                                 |
| Machines           | Any SSH host with live stats, files, containers and power controls, using your existing SSH config                                                             |
| Media and calendar | Every MPRIS player, a local music library, and your GNOME calendars with one-tap meeting links                                                                 |
| Tools              | Clipboard history, color picker, emoji picker, keystroke display, Focus Dim and presenter blur                                                                 |
| Developer          | Database browser with confirmed writes, Site Audit, Herdr and Quinjet boards, Project Doctor and Graftcode                                                     |
| Maintenance        | apt, snap and flatpak updates in one list, review-first removal and a disk cleaner that uses the Trash                                                         |
| Top bar            | CPU and memory beside the clock and a notch with now playing, notifications, files, clipboard and camera                                                       |

Global shortcuts: `Ctrl+Alt+V` opens Veronica, `Ctrl+Alt+B` clipboard,
`Ctrl+Alt+M` mic mute, `Ctrl+Alt+P` color picker, `Ctrl+Alt+E` emoji picker and
`Ctrl+K` searches every page. Extensions can add their own, and Settings,
Shortcuts lists every one with a way to change or turn it off.

## Command line

`vr` reaches the same operations as the app:

```bash
vr diagnose
vr usage summary
vr machines ls
vr media toggle
```

Every read command takes `--json`. See [docs/COMMANDS.md](docs/COMMANDS.md) for
the full list and [docs/RUNNING.md](docs/RUNNING.md) for the notch and
troubleshooting.

## Privacy

Usage history never leaves your computer. Rate-limit checks go straight to the
provider with the token your agent CLI already holds. Veronica only uses the
network for features you run, such as update checks, site audits and remote
machines.

## Building from source

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
  libayatana-appindicator3-dev libjavascriptcoregtk-4.1-dev \
  libsoup-3.0-dev libxdo-dev patchelf
cd apps/desktop && bun install && bunx tauri build --bundles deb
```

See [docs/BUILDING.md](docs/BUILDING.md) and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for details, and
[CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request.

## License

Copyright (C) 2026 [Naman Dadhich](https://github.com/namannn04) and
[Pulkit](https://github.com/pulkitxm).

Veronica is free software under the [GPL-3.0](LICENSE) with the attribution
terms in [NOTICE](NOTICE). Anything you distribute that contains or is based on
Veronica must stay under the same license and credit the project:

> Based on Veronica by Naman Dadhich and Pulkit,
> <https://github.com/namannn04/Veronica>
