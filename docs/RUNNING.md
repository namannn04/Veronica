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

## Using the notch

The notch is the pill under the top bar, centred on the clock.

| Action | What happens |
| --- | --- |
| Hover it | Peeks open, and closes again shortly after the pointer leaves |
| Click it | Pins it open, so it stays until dismissed |
| Click it again, or the ✕ | Closes it |
| Escape | Closes it when pinned |

The pinned state is remembered, so it reopens the way you left it. Whether the
notch exists at all is the `notchShelf` extension, toggleable from
Extensions, the tray menu, or the command line:

```bash
vr config set notchShelfEnabled false   # hide it
vr config set notchShelfEnabled true    # bring it back
```

Clicks pass through everywhere the island is not, so the desktop underneath
stays usable.

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
