# Building Veronica

The packaged shell extension supports GNOME Shell 46 through 50, covering
Ubuntu 24.04 through 26.04. Its camera path feature-detects the `St.ImageContent`
signature that changed in GNOME 48, so the same package works on both sides of
that API boundary.

## Toolchain

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Bun, for the interface build
curl -fsSL https://bun.sh/install | bash

# Native dependencies
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
  libayatana-appindicator3-dev libjavascriptcoregtk-4.1-dev \
  libsoup-3.0-dev libxdo-dev patchelf
```

`jq` is a runtime dependency of the usage collector and is declared by the
package. `bun` is recommended rather than required: the collector uses it to run
`ccusage`, and falls back to a Node install when it is absent.

## Tests

```bash
cargo test --workspace          # the crates and the app
cd extension && npm test        # the shell extension's pure logic
```

The portable crates carry the interesting coverage, including parity tests that
pin the values Edith's Swift produces for the rate-limit maths.

The shell extension's tests cover the parts that can run without a shell: the
procfs arithmetic behind the top bar's CPU and memory readout, and Focus Dim's
clamps. Those live in modules that import nothing from `gi://` — `procStats.js`
and `focusDimMath.js` — precisely so they are testable; the widgets that use them
are not. `extension/package.json` exists to declare the directory as ES modules
for node and editors, and is not installed.

Focus Dim's clamps exist in both Rust and JavaScript on purpose. The Rust copy
stops a bad value being *stored*; the JavaScript copy stops a hand-edited
settings file being *applied*, which matters because an overlay at full opacity
would leave the user unable to see the desktop well enough to undo it.

## Running during development

```bash
cd apps/desktop
bun install
bunx tauri dev
```

## Release build

From a clean checkout, build the Debian package and AppImage, smoke-test the
packaged CLI and write SHA-256 checksums in one command:

```bash
./scripts/release.sh
```

Artifacts land under `target/release/bundle/`. Tauri's release hook builds
the `vr` CLI first, before compiling the interface, because the Debian bundle
ships that exact release binary. Do not build or copy `vr` separately: keeping
it inside the one release command prevents an old CLI from being packaged with
a new desktop app.

### Use `tauri build`, not `cargo build --release`

A bare `cargo build --release -p veronica-desktop` produces a binary that still
points at the Vite dev server, so every window opens on
`Could not connect to 127.0.0.1`. Only `tauri build` sets the configuration that
embeds the built interface. If a release binary shows that error, this is why.

## X11 rather than Wayland

The process asks GTK for the X11 backend before GTK starts, because the notch
overlay has to position itself at the top centre of the display and stay above
other windows. A Wayland client cannot do either: it may not place its own
toplevel, and it may not raise itself. Under XWayland — which every GNOME
session runs — both work, and the island lands where it should.

Set `VERONICA_GDK_BACKEND=wayland` to run natively on Wayland, accepting that
the notch will appear wherever the compositor puts it. An existing `GDK_BACKEND`
in the environment is always respected.

## Logging

`VERONICA_LOG` takes a `tracing` filter, and logs go to stderr:

```bash
VERONICA_LOG=debug ./target/release/veronica
VERONICA_LOG=debug vr usage refresh
```

`vr` also takes `-v`, `-vv` and `-vvv`.

## The usage collector

`resources/refresh-usage` is Edith's collector, vendored so both projects report
the same numbers. It is compiled into the binary with `include_str!` and written
to the cache directory on each launch, so an upgrade never runs a stale copy.

It carries one Linux fix, which a test asserts is still present: two sorts feeding
`comm` must both use `LC_ALL=C`, or the shell's UTF-8 collation disagrees with
`jq`'s codepoint ordering and the run aborts at the last stage. When updating the
vendored script from upstream, reapply that fix.
