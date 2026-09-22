# Running Veronica

## The short version

```bash
# 1. Install the latest verified GitHub release
curl -fsSL https://raw.githubusercontent.com/namannn04/Veronica/main/install.sh | bash

# 2. Launch it
veronica              # or find "Veronica" in Activities

# 3. Collect your agent usage the first time
vr usage refresh --progress
```

The app puts an icon in the top bar. Left-click opens the window; right-click
gives a menu with Refresh usage, Toggle notch, and Quit.

## The top bar

Veronica replaces only the center date button and its popup with the compact
Edith-style shelf. It does not inject agent data into GNOME's large calendar
dropdown and does not add a spend indicator to the right side. Ubuntu Quick
Settings remains the single owner of Wi-Fi, Bluetooth, volume and battery.

The Debian package installs the extension system-wide. Veronica's Settings →
General page reports whether it is active and can enable it when GNOME already
knows about the installation. Because GNOME Shell only
scans for extensions at startup, and Wayland has no way to restart the shell in
place, a **fresh login is required once** after installing:

```bash
# after installing the package, log out and back in, then:
gnome-extensions enable veronica@namannn04.github.io
gnome-extensions info veronica@namannn04.github.io   # expect State: ACTIVE
```

From a source checkout, install it for your user instead:

```bash
./extension/install.sh
```

Left-click the tray icon to open the window; right-click for a menu with
Refresh usage and Quit.

The AppImage does not install the system-wide GNOME extension or `vr`. It can
run the desktop-only pages, but shell-owned features such as the notch,
clipboard capture, Focus Dim and keystroke display require the Debian package.

### If the old or duplicated top bar still appears

GNOME gives a user-installed extension under `~/.local/share` priority over the
system copy from the Debian package. Move an old checkout aside, then log out:

```bash
gnome-extensions disable veronica@namannn04.github.io
mv ~/.local/share/gnome-shell/extensions/veronica@namannn04.github.io \
  ~/.local/share/gnome-shell/extensions/veronica@namannn04.github.io.old
```

After the next login, enable the system copy. `gnome-extensions info` should
show its path under `/usr/share/gnome-shell/extensions/`.

### If nothing appears in the notch

- `gnome-extensions info veronica@namannn04.github.io` should say `State: ACTIVE`.
  `ERROR` means it threw, and the reason is in the journal.
- Watch the shell's own log for Veronica's messages:

  ```bash
  journalctl --user -f | grep -i veronica
  ```

  On a healthy start it logs `compact Edith notch enabled` and the path it found
  `vr` at.
- `vr at "not found"` means the CLI is not where the shell can see it. A shell
  extension does not inherit a login shell's `PATH`, so the extension checks
  `/usr/bin/vr`, `/usr/local/bin/vr` and `~/.local/bin/vr` in that order.
- On some Ubuntu installs `disable-user-extensions` is `true`, which makes the
  shell ignore *user* extensions entirely and looks identical to a broken
  extension. The packaged system-wide install is unaffected, but if you used
  `install.sh`:

  ```bash
  gsettings get org.gnome.shell disable-user-extensions   # want: false
  ```

## Compact notch

The notch is active whenever the GNOME extension is active. Home contains live
MPRIS music, usage rings, Keep Awake and Lid Awake. The bell tab embeds GNOME's
real notification list in a fixed-height section so notifications scroll rather
than stretching the popup. Files can be added, opened, removed and persist across
logins. Camera previews the real webcam inside the notch and releases it as soon
as the preview or notch closes. Clean Keys grabs and blocks the keyboard,
Presenter masks sensitive values, and Pick Color copies a GNOME-picked hex value
to the clipboard. The same five quick actions are available from the desktop app.
Disabling Veronica restores GNOME's stock date button and calendar dropdown.

## Developing the extension

Wayland cannot restart the shell in place, so a code change needs a fresh login
— or a disposable shell on its own bus, which is how this was built:

```bash
dbus-run-session -- bash -c 'echo $DBUS_SESSION_BUS_ADDRESS > /tmp/bus; \
  exec gnome-shell --headless --virtual-monitor 1400x900'
# then, against that bus:
DBUS_SESSION_BUS_ADDRESS=$(cat /tmp/bus) \
  gnome-extensions enable veronica@namannn04.github.io
```

The headless shell logs to its own stdout, which is where the extension's
messages appear. Screenshots are refused even there, so verification is by log.
Note that GJS caches ES modules per session: re-enabling is not enough to pick
up a code change, the shell has to be restarted.

## Running from source

```bash
cd apps/desktop
bun install
bunx tauri dev
```

`tauri dev` starts the Vite dev server and the app together, with hot reload for
the interface.

To build a release package instead:

```bash
cd apps/desktop && bunx tauri build --bundles deb
```

The Tauri release hook rebuilds `vr` before bundling it, so one command produces
a desktop app and CLI from the same checkout. The packaging test pins that hook
to prevent a stale binary from silently returning.

To update an existing installation with the package you just built:

```bash
sudo apt install --reinstall ./target/release/bundle/deb/Veronica_0.1.10_amd64.deb
```

## Backup and restore

Settings › Backup can export to Downloads, inspect an existing archive, and
restore it after explicit confirmation. The archive includes Veronica's
configuration, persistent data, and state; disposable cache and runtime files
are deliberately excluded. Imports verify every path, size, and SHA-256 digest
before writing anything.

The same flow is available without opening the app:

```bash
vr backup export
vr backup inspect ./Veronica-backup-2026-08-27.veronica-backup
vr backup import ./Veronica-backup-2026-08-27.veronica-backup --confirm
```

## Alerts

Alerts are off until you ask for them, and while they are off Veronica makes no
request to any provider on their behalf.

```bash
vr config set notifyMaster true   # or Settings › Alerts
vr usage alerts                   # a dry run: what a poll now would post
vr alerts test                    # post one banner, to check they arrive
vr alerts state                   # what the notifier is comparing against
```

Every alert is edge-triggered: it fires when a window *crosses* a level or a
pacing zone, not while it sits there. So the usual reason an expected alert did
not appear is that the crossing already happened — `vr alerts state` shows the
level being compared against, and `vr alerts clear` forgets it so the next poll
starts from where you are now. That is also worth doing after changing a
threshold, since the stored comparison is then against the old scale.

## Presenter mode

```bash
vr presenter enable
vr presenter status        # the blur state, and what detection sees
vr presenter start         # blur now; stop, dismiss and resume also exist
vr presenter blur calendar off   # reveal one category deliberately
```

Detection watches the compositor's screencast sessions, which is the path every
well-behaved screen share on Wayland takes. If presenter mode did not activate
during a call:

```bash
cargo run -p veronica-system --example detect-share
```

That prints the session counts, or says why detection could not run at all — a
desktop with no Mutter reports "cannot tell" rather than a quiet screen, so
presenter mode still works by hand there.

`dismiss` applies to the share happening now; once it ends, the next one blurs
again. Turning detection off entirely is `vr config set presenterAutoEnabled
false`.

## Focus Dim and the top bar readout

Both are drawn by the shell extension, because only the compositor can place a
dim behind another application's window or draw live text in the top bar.

```bash
vr focus-dim on
vr focus-dim intensity 60          # a percentage, capped at 90
vr focus-dim mode dimUnfocused     # or perScreenFront
vr config set menuBarSystemStats true   # CPU and memory beside the clock
```

Both react as soon as the setting changes: the extension watches the settings
file rather than polling, so there is no delay and no subprocess.

## Troubleshooting

**Every window says "Could not connect to 127.0.0.1".** The binary was built
with `cargo build --release` rather than `tauri build`, so it still expects the
dev server. Rebuild with `bunx tauri build`, or start the dev server.

**The build fails with a path that no longer exists.** `target/` caches absolute
paths. After moving the checkout, run `cargo clean`.

**Push updates stop arriving** — usage finishing, notifications appearing, the
collector's progress. Check `apps/desktop/src-tauri/capabilities/default.json`
still grants `core:default` to both windows. Custom commands are not gated by
capabilities, but the event API is, so without it `listen()` is denied and the
interface silently stops updating while everything else keeps working.

**The notch appears in the wrong place, or behind other windows.** It needs the
X11 backend to position and raise itself, which the process requests before GTK
starts. If `GDK_BACKEND` is already set to `wayland` in your environment, that
choice is respected and the compositor decides the placement instead.

**No notifications in the notch.** Veronica reads them by watching the session
bus, which needs the bus to permit monitoring. `vr diagnose` reports the session
it resolved; the feature is absent rather than fatal if monitoring is refused.

**"cannot put text on the clipboard".** Writing the clipboard on Wayland needs
the compositor, so Veronica asks its shell extension. The message names every
route it tried. If it says the extension has "no such method", the installed
extension predates the feature: reinstall it and log out and back in, since
Wayland cannot reload a changed extension in place. `wl-clipboard` or `xclip`
also work as fallbacks.

**Focus Dim or the top bar readout does nothing.** Both live in the extension, so
a stale install has neither. `gnome-extensions list | grep veronica` confirms it
is present; a changed extension needs a fresh login. On a desktop that is not
GNOME, `vr focus-dim status` says so rather than pretending the switch worked.

**The colour picker never returns.** It waits for a click and is cancelled with
Escape; cancelling exits non-zero with a plain message rather than recording a
colour. A pick abandoned entirely times out after five minutes.

## Logging

```bash
VERONICA_LOG=debug veronica     # or: VERONICA_LOG=debug vr usage refresh
```

Logs go to stderr so stdout stays a single JSON document for the CLI.
