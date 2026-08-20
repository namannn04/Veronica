#!/usr/bin/env bash
# Install the Veronica GNOME Shell extension for the current user.
#
# The Debian package installs it system-wide, which is preferable because a
# system extension is unaffected by the disable-user-extensions setting. Use
# this only when running from a source checkout.
set -euo pipefail

UUID="veronica@namannn04.github.io"
SOURCE="$(cd "$(dirname "$0")" && pwd)"
DEST="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions/$UUID"

mkdir -p "$DEST"
install -m 644 "$SOURCE/metadata.json" "$SOURCE/extension.js" \
               "$SOURCE/lib.js" "$SOURCE/stylesheet.css" "$DEST/"
echo "installed to $DEST"

# A user extension is ignored entirely when this is true, and it silently is on
# some installs, which looks exactly like a broken extension.
if [ "$(gsettings get org.gnome.shell disable-user-extensions)" = "true" ]; then
  echo "enabling user extensions (was disabled)"
  gsettings set org.gnome.shell disable-user-extensions false
fi

echo
echo "Now log out and back in, so GNOME Shell picks it up, then run:"
echo "  gnome-extensions enable $UUID"
echo
echo "On Wayland the shell cannot be restarted in place, so a fresh login is"
echo "the only way to load a newly installed extension."
