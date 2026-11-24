#!/usr/bin/env bash
set -euo pipefail

UUID="veronica@namannn04.github.io"
SOURCE="$(cd "$(dirname "$0")" && pwd)"
DEST="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions/$UUID"

mkdir -p "$DEST"

DEV_ONLY=("package.json")

shopt -s nullglob
files=()
for candidate in "$SOURCE"/*.js "$SOURCE"/*.json "$SOURCE"/*.css "$SOURCE"/*.png; do
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

for installed in "$DEST"/*.js "$DEST"/*.json "$DEST"/*.css "$DEST"/*.png; do
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
