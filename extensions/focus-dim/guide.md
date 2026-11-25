# Focus Dim

Focus Dim darkens everything behind the window you are working in. Veronica's
GNOME Shell extension draws the overlay, since only the compositor can place a
dim behind another app's window. Turning the extension on turns dimming on.

## Commands

- `vr focus-dim status` reports the settings and whether this desktop can
  apply them.
- `vr focus-dim on` and `vr focus-dim off` turn dimming on and off.
- `vr focus-dim intensity 45` sets how dark to go, from 0 to 90 percent.
- `vr focus-dim fade 0.25` sets how long the fade takes.
- `vr focus-dim mode dimUnfocused` dims displays other than the focused one;
  `perScreenFront` dims behind the front window on each display.

Every command accepts `--json`.
