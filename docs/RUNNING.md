# Running Veronica

## The short version

```bash
# 1. Install the built package
sudo apt install ./target/release/bundle/deb/Veronica_0.1.0_amd64.deb

# 2. Launch it
veronica              # or find "Veronica" in Activities

# 3. Collect your agent usage the first time
vr usage refresh --progress
```

The app puts an icon in the top bar. Left-click opens the window; right-click
gives a menu with Refresh usage, Toggle notch, and Quit.

## The top bar

Veronica does not draw a panel of its own. It adds sections to the top bar's
real clock dropdown — the one that already shows notifications, media and the
calendar — through a GNOME Shell extension. Whatever the shell already does
well is left alone; Veronica contributes what the shell has no idea about:
agent usage and spend, and machine state. A small indicator also appears in the
status area on the right, showing today's spend.

The Debian package installs the extension system-wide. Because GNOME Shell only
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

### If nothing appears in the dropdown

- `gnome-extensions info veronica@namannn04.github.io` should say `State: ACTIVE`.
  `ERROR` means it threw, and the reason is in the journal.
- Watch the shell's own log for Veronica's messages:

  ```bash
  journalctl --user -f | grep -i veronica
  ```

  On a healthy start it logs `added sections to the clock dropdown` and the path
  it found `vr` at.
- `clock dropdown layout not recognised` means the shell's popup changed shape
  and the sections fell back to a plain menu item. The extension looks for the
  actor with style class `datemenu-calendar-column`; that is what to re-check
  against a newer GNOME.
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
cargo build --release -p veronica-cli    # the vr binary the package ships
cd apps/desktop && bunx tauri build --bundles deb
```

Both steps are needed: the bundler copies `target/release/vr` into the package,
so a clean checkout must build the CLI first.

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

## Logging

```bash
VERONICA_LOG=debug veronica     # or: VERONICA_LOG=debug vr usage refresh
```

Logs go to stderr so stdout stays a single JSON document for the CLI.
