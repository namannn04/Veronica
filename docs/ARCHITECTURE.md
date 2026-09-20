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
| `crates/veronica-core` | XDG paths, capability model, extension catalogue, settings, presenter, Focus Dim and Keystroke Highlight models, the emoji catalogue and its ranking, timed awake sessions, clipboard and swatch history. No GUI or toolkit dependency. |
| `crates/veronica-usage` | Collector driver, `usage.json` schema 8 decoding, dashboard rollups, rate-limit maths, alert decisions, share cards. |
| `crates/veronica-system` | procfs metrics, logind inhibitors, PipeWire audio and the per-application mixer, BlueZ, apt/snap/flatpak maintenance, D-Bus notifications, portal probing, screen colour sampling, clipboard writing and pasting in place, screen-share detection. |
| `crates/veronica-media` | MPRIS control and local playback. |
| `crates/veronica-calendar` | Agenda from GNOME's calendar server, join links from Evolution Data Server. |
| `crates/veronica-machines` | The fleet: host model, the probe, and running it locally or over SSH. |
| `crates/veronica-audit` | Site Audit: sitemap discovery, page metadata, and Edith's eleven checks. |
| `crates/veronica-database` | The database client: connection definitions, the capability map, six adapters, the confirmation guard, the SQLite metadata store, and the read-only MCP server. |
| `crates/veronica-cli` | The `vr` binary. |
| `apps/desktop` | Tauri application: Rust commands plus the React interface. |
| `resources/refresh-usage` | The bundled usage collector. |
| `extension/` | GNOME Shell extension: Veronica's sections inside the real top bar and its clock dropdown, the top bar's CPU/memory readout, clipboard capture, Focus Dim's overlay, Keystroke Highlight and pasting in place — everything only the compositor can do. |
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

`veronica-core::Capabilities` resolves each of Edith's capabilities to one of
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
- **Keystroke observation** (Keystroke Highlight) — a compositor hands key
  presses only to the focused window, so showing them on screen means reading
  them from inside the compositor. Veronica's extension does, listen-only: it
  never consumes an event, and records nothing while the shell holds a modal
  grab, which is where passwords are typed.
- **Paste in place**, and the emoji picker's insert-in-place — synthesising
  input into a window Veronica does not own is the same problem in reverse. The
  extension sends one Ctrl+V through a virtual keyboard on the seat, which needs
  no per-session portal approval.

Everything else has a working Linux route: PipeWire for audio, the
per-application mixer and mic mute, BlueZ for Bluetooth, logind for
prevent-sleep and lid-awake, MPRIS for media, Evolution Data Server for
calendar, and the desktop portal for colour picking, camera, global shortcuts
and screen-share detection. `vr diagnose` prints the resolved state
with the backend each one talks to.

Running headless — `vr` over SSH — marks the display-dependent capabilities
`unsupported` rather than letting them fail later.

## The database guard

Edith puts an XPC broker between the app and a database, because a sandboxed
macOS app cannot hold a socket itself. Veronica has no sandbox to cross, so the
boundary is `veronica-database::session::Session` instead — and the safety it
enforces is the same, because the safety was never the process boundary.

A change is described as a *plan*: what it does, to what, how far it reaches,
whether it can be undone. `preview` validates the plan against the connection's
policy, measures the impact against the server, derives the warnings it earns,
and returns a token:

```
base64url(payload) . base64url(HMAC-SHA256(payload))
```

The payload holds two keyed digests — one over what would *run* (the plan plus
the connection's policy), one over what was *shown* (the redacted request, the
warnings, the required confirmation). `apply` verifies the signature, checks the
clock, recomputes both digests from the plan it is handed, and requires them to
match. Editing anything between the two steps invalidates the token, and nothing
is sent.

Three further properties: issuing a preview registers a receipt that authorising
consumes, so a token works exactly once even with the app and the CLI both
running — the `DELETE ... RETURNING` in the store is what makes that atomic. A
preview expires. And the signing key lives in the keyring, so a token cannot be
forged without it.

Values never reach a statement. Identifiers are quoted by the product's own
rules, values are bound as parameters, and a preview shows a parameter's *type*
rather than its value — because a preview is printed, logged and stored, and the
value being written may be the password being rotated.

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

The shell extension's pure logic is tested too, with `cd extension && npm test`.
Anything testable is kept in a module that imports nothing from `gi://`, so it
runs under plain node: `procStats.js` holds the procfs arithmetic behind the top
bar readout, `focusDimMath.js` holds Focus Dim's clamps, and
`keystrokeLabels.js` holds Keystroke Highlight's label resolution and queue.

Where a rule is enforced on both sides — Focus Dim's clamps, Keystroke
Highlight's duration and queue — the Rust and JavaScript tests are written to
mirror each other. The two copies guard different moments (the core stops a bad
value being *stored*; the extension stops a hand-edited file being *applied*)
and must not drift.

Two things are worth verifying against the running desktop rather than in a test,
because they depend on the compositor:

```bash
cargo run -p veronica-system --example detect-share   # what presenter mode sees
vr alerts test                                        # that banners arrive
```

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

## Alerts

`veronica-usage/src/alerts.rs` is a port of Edith's `LimitNotifierLogic`: the same
five kinds of alert, the same edge-triggering, the same wording. Two things had to
change, both because freedesktop notifications are not
`UNUserNotificationCenter`:

- **Reminders are fired, not scheduled.** macOS lets Edith hand a future
  notification to the OS. There is no equivalent here, so the poll fires the
  reminder when its moment arrives, and the persisted state records which reset
  instant has already been covered — otherwise a thirty-second poll would fire it
  repeatedly for one window.
- **Banners are replaced by numeric id.** macOS reuses a string identifier; the
  freedesktop server hands back a `u32`. The state remembers the last id per
  alert, so a rising threshold updates one banner instead of stacking five.

The runner is `apps/desktop/src-tauri/src/alerts.rs`. It reads Claude's limits
directly rather than through the gauge collector, because the alerts watch those
two windows specifically, as Edith's notifier does. While the master switch is off
it makes no provider request at all, which is why a fresh install is silent and
costs nothing.

The notifier state is persisted under `state/alerts.json`. Losing it is harmless —
it re-alerts once — so a missing or corrupt file reads as a fresh state rather
than a failure.

`vr usage alerts` is a dry run: it loads the same state and settings and reports
what a poll at this instant would post, without writing anything. That is the
thing to reach for when an alert did not fire when expected.

## Presenter mode

Three switches, not one: `presenterEnabled` (does the feature exist),
`presenterMode` (the user's own toggle) and `presenterAutoActive` (Veronica
noticed a share). The gate is Edith's, unchanged:

```
active = enabled && (manual || auto_active)
```

Collapsing them would mean a detected share could not be dismissed without also
turning the feature off.

Detection is the only part that differs from macOS. Edith watches for a display
being captured or mirrored; on Ubuntu the equivalent signal is an active
compositor screencast session. Every well-behaved sharing path on Wayland — a
browser tab, Zoom, OBS, GNOME's own recorder — goes through xdg-desktop-portal,
which asks Mutter for a session, and Mutter exports one object per live session
under a collection:

```
/org/gnome/Mutter/ScreenCast/Session/u8
/org/gnome/Mutter/RemoteDesktop/Session/u3
```

So the count of that collection's children is the number of sessions.
Introspecting the *service root* is not enough: it gains a single `Session` child
whatever the number beneath it, which would under-report every time.
`veronica-system/src/screencast.rs` reads the collection, and a test pins that
distinction so the shortcut is not reintroduced.

The result is written to the settings rather than held in memory, because three
processes need it: the app blurs its own figures, the shell extension blurs the
notch's, and `vr presenter status` reports it. One file they all read is the only
arrangement in which those three cannot disagree.

`cargo run -p veronica-system --example detect-share` prints what detection sees,
which is what to run when presenter mode did not activate during a call.

## Focus Dim and the compositor

Edith stacks its own translucent windows beneath the focused one. A Wayland
client cannot: it may not position itself, raise itself, or learn what else is on
screen. The compositor can, so on Ubuntu the overlay is drawn by the shell
extension — one actor per monitor inside `global.window_group`, with the window
that stays bright raised above it.

That makes the capability depend on the *shell* rather than on the display
protocol: available on GNOME Wayland, where no client could do it, and
unavailable on a non-GNOME X11 desktop, where a client could but nothing has been
written. `capabilities.rs` resolves it that way and a test pins it.

The overlay is inside the window group rather than over the stage, so the panel,
the overview and Veronica's own notch are never covered.

## Honest capabilities

A capability with no implementation behind it resolves to `IntegrationRequired`
with a reason, never `Available`. Companion is now implemented locally with an
atomic XDG-data index and PipeWire recordings, so it correctly resolves as
available without a container runtime. Tests pin both the implementation state
and the no-Docker behavior.

The extension catalogue's `defaults_key` is serialised to the interface for the
same reason: the settings key each extension toggles is the catalogue's to know,
and a copy of the fifteen keys in TypeScript would drift the first time one was
added.

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

Rate-limit figures cannot be derived locally: only the provider knows them. The
request goes straight from the machine to the provider using the token the
agent's own CLI already holds; Veronica never proxies it. Other networked
features are explicit in their purpose — update checks, site audits, remote
machines and databases, and requested tool installs — while local usage history
and telemetry never leave the machine.

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

## Notch clock replacement

The extension replaces only GNOME's center date button and its popup with the
compact Edith shelf. Ubuntu Quick Settings remains the single owner of network,
Bluetooth, volume and battery:

- The Notifications tab reuses GNOME's `CalendarMessageList` class but constrains
  it to the shelf height, so it scrolls instead of creating the stock two-column
  calendar popup shown beside the shelf.
- `extension/notchPanel.js` owns the Edith-style surface. Home places
  now-playing and two usage rings side by side with quick actions below;
  Clipboard provides copy and delete interactions.
  `St.DrawingArea` and Cairo draw the percentage rings (`extension/ring.js`).
  Files and Camera remain visible with explicit backend status
  until their Linux implementations exist, avoiding the old dead-toggle state.

- `extension/themes.js` is the single list of appearances the notch knows.
  Both the panel, which resolves the `appearance` setting into a style class,
  and the clock, which removes the previous class before adding the next one,
  read it. Two hand-kept copies of that list is the shape this replaced: adding
  a theme to one and not the other left the old class stuck on the popup. The
  same list exists in `apps/desktop/src/lib/preferences.ts`, in the family
  selector lists in `apps/desktop/src/styles.css` and in
  `veronica_core::appearance`, and `crates/veronica-core/tests/themes.rs`
  compares all four.

Three things about it are deliberate:

- **The notch follows extension state.** Enabling the extension shows it;
  disabling the extension restores the stock date menu. There is no second mode
  that injects Veronica sections into GNOME's calendar dropdown.
- **The stock clock is hidden, never destroyed.** `dateMenu` is set invisible
  and nothing more, so disabling the extension restores it with
  no lost state.
- **Quick Settings is never touched.** Keeping GNOME's native status controls
  avoids duplicate Wi-Fi, Bluetooth, speaker and battery icons and preserves
  the complete Ubuntu system menu.

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

`org.gnome.Shell.Eval` over D-Bus (the shell needs `--unsafe-mode` for this to
return anything) can then walk the live actor tree — `Main.panel.statusArea`,
a button's `.menu.box` — to confirm what actually got built, rather than
inferring it from logs alone.

One more thing this process caught: `gnome-extensions disable` followed by
`enable` inside the *same* shell process does not reliably pick up a change to
one of the extension's own submodules (confirmed directly — a fix to
`notchClock.js` was invisible after a disable/enable cycle, and only took
effect after the whole `gnome-shell` process was restarted). GJS caches ES
modules by URL for the life of the process, and disable/enable does not evict
that cache. A code change to any file under `extension/` needs the whole
shell restarted to be certain it took — on Wayland that means logging out and
back in; there is no in-session substitute for real testing.
