# Veronica Architecture

Veronica is a native control center for Ubuntu. It is a port of
[Edith](https://github.com/pulkitxm/edith), a macOS control center, aiming at the same feature set with
Linux-native implementations rather than a compatibility layer.

## Why not port the Swift directly

Edith is ~116K lines of Swift across 520 files: SwiftUI for presentation and
AppKit for system integration. Swift compiles on Linux, but neither SwiftUI nor
AppKit exists there, so 122 view files and 114 AppKit service files have no
target to compile against. An earlier attempt (the upstream `nd-ubuntu-fixes1` branch) took this route and had to delete 23K lines to reach a
build; its own notes record the result as "every extension reports
`unavailable`, and `supportedCapabilities` is empty".

Veronica instead keeps Edith's *models* — the capability map, the extension
catalogue, the rate-limit maths, the usage schema — and reimplements the
platform layer against Linux services.

## Stack

| Layer | Choice | Why |
| --- | --- | --- |
| System integration | Rust | Direct access to procfs, D-Bus (`zbus`), PipeWire, SSH and Docker, with no runtime |
| Presentation | Tauri v2 + React/TypeScript | Uses the system WebKitGTK, so the package stays small; the dashboard, charts and heatmap are far cheaper to build than in a widget toolkit |
| CLI | Rust, shared crates | `vr` reaches the same operations as the UI, exactly as Edith's `ed` does |
| Usage collection | Edith's `refresh-usage` script, verbatim | Identical numbers on both platforms, see below |

## Crate layout

| Path | Responsibility |
| --- | --- |
| `crates/veronica-core` | XDG paths, capability model, extension catalogue, settings. No GUI or toolkit dependency. |
| `crates/veronica-usage` | Collector driver, `usage.json` schema 8 decoding, dashboard rollups, rate-limit maths. |
| `crates/veronica-system` | procfs metrics, logind inhibitors, PipeWire audio, D-Bus notifications, portal probing. |
| `crates/veronica-media` | MPRIS control and local playback. |
| `crates/veronica-calendar` | Agenda from GNOME's calendar server, join links from Evolution Data Server. |
| `crates/veronica-cli` | The `vr` binary. |
| `apps/desktop` | Tauri application: Rust commands plus the React interface. |
| `resources/refresh-usage` | The bundled usage collector. |
| `extension/` | GNOME Shell extension: Veronica's sections inside the real top bar and its clock dropdown. |
| `packaging/` | Debian package, AppImage, desktop entry and AppStream metadata. |

## The usage collector is shared, not reimplemented

Edith collects agent usage with a bundled bash + jq script that walks
`~/.claude`, `~/.codex`, `~/.cursor`, `~/.commandcode` and opencode's database.
It was already partly Linux-aware (XDG cache fallback, `md5sum` fallback).

Veronica ships that script verbatim apart from one fix. On Linux the script
aborted at `comm -23`, because one input was sorted under the shell's UTF-8
collation while the other came from `jq`'s codepoint ordering, and `comm`
compares bytes. Forcing `LC_ALL=C` on both sorts makes the orderings agree:

```
jq -r '...' | LC_ALL=C sort      >"$TMP/cwds.txt"
jq -r '.cwd' | LC_ALL=C sort -u  >"$TMP/cwds-have.txt"
```

Reusing the script means the figures are identical to Edith's on the same
machine, and there is one collector to maintain rather than two. A test asserts
the fix is still present in the bundled copy, because losing it breaks
collection at the last stage with a confusing error.

## Capabilities decide what is offered

`veronica-core::Capabilities` resolves each of Edith's 24 capabilities to one of
`available`, `permissionRequired`, `integrationRequired` or `unsupported` for
the running session, and the extension catalogue derives availability from
that. Nothing is hard-coded per platform in the UI; a feature appears as
degraded or unavailable with a reason attached.

Resolution depends on the session, because a Wayland compositor deliberately
withholds capabilities that X11 grants:

- **Clipboard history** — a Wayland compositor only hands the clipboard to the
  focused window, so continuous background history needs help from the shell.
- **Window dimming** (Focus Dim) — dimming other windows is the compositor's
  job and cannot be done from outside it.
- **Input suppression** (keyboard lock) — needs an exclusive evdev grab, which
  needs membership of the `input` group.

Everything else has a working Linux route: PipeWire for audio and mic mute,
logind for prevent-sleep and lid-awake, MPRIS for media, Evolution Data Server
for calendar, and the desktop portal for colour picking, camera, global
shortcuts and screen-share detection. `vr diagnose` prints the resolved state
with the backend each one talks to.

Running headless — `vr` over SSH — marks the display-dependent capabilities
`unsupported` rather than letting them fail later.

## Paths

Edith keeps everything under one Application Support directory. Veronica
follows the XDG base directory specification, so configuration, data, cache,
state and runtime are separate roots, and every XDG variable is honoured with
the spec's fallbacks. `XDG_RUNTIME_DIR` has no spec-defined default, so it
falls back under the cache directory.

## Testing

The portable crates are unit tested, with parity tests pinning the values
Edith's Swift produces for the rate-limit maths — the smoothstep ramps, the
risk blend, the zone hysteresis and the budget states. Run `cargo test
--workspace`.

## The top bar

GNOME's top bar and its clock dropdown belong to the shell. An application
window cannot be stacked above them, cannot reserve space beside them, and on
Wayland cannot even place itself. Drawing a look-alike panel underneath the real
one was tried and abandoned: it reads as a second bar rather than part of the
desktop.

So the top bar integration is a GNOME Shell extension, which runs inside the
shell's own process and can add to its widgets directly. It contributes only
what the shell has no notion of — agent usage, spend, machine state — and leaves
the notifications, media controls and calendar to the shell, which already does
them well.

The extension holds no domain logic. Every figure it shows comes from
`vr ... --json`, so the panel and the application cannot disagree about a
number, and the extension stays small enough to audit. It finds the dropdown's
right-hand column by the shell's own style class, `datemenu-calendar-column`,
rather than by private field names, which move between releases without notice.
