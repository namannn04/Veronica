# Notch Tools

Notch Tools backs the image and QR panels in the notch. Everything happens on
this computer; previews and exports are kept privately in the cache and only
the newest few are kept.

## Commands

- `vr utility image <path> [png|jpg|svg]` reads an image, makes a small
  preview and, when a format is given, converts it.
- `vr utility qr <text>` renders a QR code as a PNG.

Every command accepts `--json`.
