# Building Veronica

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
cargo test --workspace
```

The portable crates carry the interesting coverage, including parity tests that
pin the values Edith's Swift produces for the rate-limit maths.

## Running during development

```bash
cd apps/desktop
bun install
bunx tauri dev
```

## Release build

```bash
cd apps/desktop
bunx tauri build --bundles deb
```

The package lands in `target/release/bundle/deb/`.

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
