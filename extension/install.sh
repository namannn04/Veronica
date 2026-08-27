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

# Every shipped file, by extension rather than by name. An explicit list goes
# stale the moment a module is added, and a missing module is invisible until
# the shell fails to import it at login.
# package.json is here so node and editor tooling read this directory as ES
# modules; GNOME only loads metadata.json and the modules extension.js imports.
DEV_ONLY=("package.json")

shopt -s nullglob
files=()
for candidate in "$SOURCE"/*.js "$SOURCE"/*.json "$SOURCE"/*.css; do
  name="$(basename "$candidate")"
  skip=""
  for dev in "${DEV_ONLY[@]}"; do
    [ "$name" = "$dev" ] && skip=1
  done
  [ -n "$skip" ] || files+=("$candidate")
done
if [ ${#files[@]} -eq 0 ]; then
  echo "no extension files found in $SOURCE" >&2
  exit 1
fi
install -m 644 "${files[@]}" "$DEST/"

# A file removed from the source tree must not linger in an old install, where
# it would keep being imported by a stale sibling.
for installed in "$DEST"/*.js "$DEST"/*.json "$DEST"/*.css; do
  [ -e "$installed" ] || continue
  name="$(basename "$installed")"
  keep=""
  for wanted in "${files[@]}"; do
    [ "$(basename "$wanted")" = "$name" ] && keep=1
  done
  if [ -z "$keep" ]; then
    echo "removing stale $name"
    rm -f "$installed"
  fi
done

echo "installed ${#files[@]} files to $DEST"

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
