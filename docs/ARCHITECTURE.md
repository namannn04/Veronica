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
| `crates/veronica-machines` | The fleet: host model, the probe, and running it locally or over SSH. |
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

## Reaching other machines

Remote machines are probed by running the `ssh` binary rather than linking an
SSH library. That is a deliberate trade: it means the user's own configuration
applies unchanged — aliases, keys, agent, jump hosts, known-hosts — so a machine
that works in a terminal works here with no further setup, and Veronica never
handles a private key or a passphrase. `BatchMode=yes` is always passed, because
a host that wants a password would otherwise block on a prompt nobody can see,
which looks like a hang rather than a configuration problem.

One shell snippet gathers everything in a single round trip, since over SSH each
extra command costs another connection's latency. It reads only procfs and `df`,
so it needs no privileges and nothing installed on the far end beyond a POSIX
shell. Local and remote machines run the same snippet through the same parser,
so a remote machine reports exactly what a local one does.

CPU is the one figure that cannot come from a single reading: `/proc/stat` holds
cumulative counters, so the snippet takes two samples with a short sleep between
them and the parser works out the difference. A single sample reports zero rather
than a fabricated number, and a counter reset between samples — a reboot — is
detected instead of producing nonsense.

## The clipboard

Reading the clipboard is the clearest case of a feature that a Wayland session
simply will not grant an ordinary application: only the focused window may see
the selection, so no background process can keep a history. Setting it has the
same restriction from the other side — a client needs a serial from a recent
input event, which is why a windowless process cannot put anything on the
clipboard at all.

Both halves therefore live in the shell extension, which runs inside the
compositor. It watches `owner-changed` on the display's selection, reads the
text with `St.Clipboard`, and pipes it to `vr clipboard record` on stdin — on
stdin specifically, so no shell quoting is involved and the content can be
anything, including newlines and quotes. Writing back uses
`St.Clipboard.set_text`, and a short grace window after each write stops the
extension re-recording what it just placed there.

The history itself is ordinary domain logic in `veronica-core`: deduplication
that promotes a repeat rather than adding a row, a cap on both entry count and
entry size, atomic saves, and search. The application reads that history
directly and copies back through the browser clipboard API, which is permitted
there because it happens in response to a click in a focused window.

## Rate limits and credentials

Rate-limit figures cannot be derived locally: only the provider knows them. So
this is the one place Veronica makes a network request, and it goes straight from
the machine to the provider using the token the agent's own CLI already holds.

Claude's come from its usage endpoint over HTTPS. Codex's come from
`codex app-server` over a JSON-RPC conversation on stdio, so there is no network
call and no token for Veronica to handle at all; the two windows it reports are
told apart by their duration rather than by the `primary`/`secondary` naming,
which says nothing about which is which.

Three decisions about the credential file are worth recording, because it belongs
to Claude Code rather than to Veronica:

- **The token is a header, never an argument.** Process argument lists are
  world-readable through `/proc`, so shelling out to `curl` with a bearer token
  would expose it to every other process on the machine. That is the main reason
  a real HTTP client is linked in rather than reusing the shell tooling the
  collector already depends on.
- **A refresh is written back, and that is the safer choice.** The provider may
  rotate the refresh token; keeping the old one after a rotation would invalidate
  the user's login. Writes preserve every field the file had, including ones
  Veronica knows nothing about, are atomic, and create the temporary file
  owner-only from the outset rather than tightening permissions after the secret
  is already on disk.
- **Nothing is refreshed that does not need to be.** With a minute of leeway, a
  token that is still valid is used as-is, so an ordinary read never touches the
  file at all.

The gauge itself lives in one place, `veronica-usage::gauges`, which the CLI, the
application and the shell extension all read. A ring in the top bar therefore
cannot disagree with the same figure on the dashboard.

## Full top-bar replacement

The top-bar extension has a second, optional layer beyond the clock-dropdown
sections: it can replace the whole of GNOME's own top-bar chrome with
Veronica's — the clock and its calendar/notification popup, and the network,
Bluetooth, volume and battery cluster — built from the same widgets and
libraries the stock ones use rather than reimplemented on top of something
else:

- The clock's popup reuses GNOME's own `Calendar`, `CalendarMessageList` and
  `DBusEventSource` classes from
  `resource:///org/gnome/shell/ui/calendar.js` — the exact widgets the stock
  dropdown is built from. The calendar and notification list are the real
  thing, not a reimplementation; what Veronica adds beside them (agent usage
  and rate limits, now-playing, clipboard history, machine state) is what the
  shell has no notion of.
- Network, Bluetooth, volume and battery come from `NM`, BlueZ over D-Bus,
  `Gvc` (the same PipeWire binding gnome-shell's own status/volume.js uses),
  and `UPowerGlib` respectively.

This is the highest-risk piece of the top bar integration — it touches
indicators and chrome the user relies on for basic system state — so three
things about it are deliberate:

- **Off by default, gated by a setting Veronica already reads elsewhere**
  (`topBarReplacement` in `veronica-core::Settings`), rather than activating the
  moment the extension is enabled. `vr config set topBarReplacement true`
  turns it on; `false` turns it off. The extension polls the setting every ten
  seconds rather than requiring a restart to notice a change.
- **The stock chrome is hidden, never destroyed.** `Main.panel.statusArea.aggregateMenu`
  and `dateMenu` are set invisible and nothing more; disabling the replacement,
  disabling the extension entirely, or even the extension crashing all leave
  those actors intact, so GNOME's own clock and icons reappear exactly as they
  were with one flag flip and no lost state.
- **Each piece fails independently.** The status cluster and the notch clock
  are built in separate `try`/`catch` blocks, and within the status cluster
  each of the four indicators is its own — one missing binding (no Bluetooth
  adapter, no UPower battery) or one construction failure never takes down
  anything else.

Configuring an actual connection — joining a new wifi network, pairing a
Bluetooth device — is deliberately not reimplemented; clicking an indicator
opens the matching GNOME Settings panel, which already does that well.

Built and verified against a disposable headless shell before ever reaching a
real session, the same way the rest of the extension was: real readings only,
nothing simulated. That process caught a genuine bug before it shipped — `Gvc`
signals `default-sink-changed` with `id = -1` while the default sink is still
resolving, and passing that through to `lookup_output_id`, which expects a
`uint32`, threw on marshalling. The fix follows gnome-shell's own volume
indicator: ask the control for `get_default_sink()` directly rather than
trusting the signal's argument.

### Testing this without touching a real session

A disposable `gnome-shell --headless` instance on its own D-Bus session does
**not** isolate it from the user's real state by default, in two ways this
work ran into directly: GSettings/dconf is a per-user database rather than
per-session-bus, so `gnome-extensions enable` against the disposable shell
still wrote to the real `enabled-extensions`; and `vr`'s own settings are a
plain file under `XDG_CONFIG_HOME`, which the dconf fix does not cover at all.
Verifying this feature safely needed `GSETTINGS_BACKEND=memory` plus all four
XDG directories overridden to a scratch path for the disposable shell's own
process — inherited by every `vr` subprocess the extension spawns — and the
extension files copied into that scratch data directory, since once
`XDG_DATA_HOME` no longer points at the real one, GNOME silently falls back to
whatever is installed system-wide rather than the code being tested.
