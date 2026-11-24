#!/bin/sh
set -eu

destination="$1"
config=apps/desktop/src-tauri/tauri.conf.json

install -Dm755 target/release/veronica "$destination/usr/bin/veronica"
jq -r '.bundle.linux.deb.files | to_entries[] | "\(.key)\t\(.value)"' "$config" |
  while IFS="$(printf '\t')" read -r target source; do
    install -Dm644 "apps/desktop/src-tauri/$source" "$destination$target"
  done
chmod 755 "$destination/usr/bin/vr"
install -Dm644 packaging/linux/veronica.desktop "$destination/usr/share/applications/Veronica.desktop"
for size in 32x32 128x128; do
  install -Dm644 "apps/desktop/src-tauri/icons/$size.png" "$destination/usr/share/icons/hicolor/$size/apps/veronica.png"
done
install -Dm644 apps/desktop/src-tauri/icons/icon.png "$destination/usr/share/icons/hicolor/512x512/apps/veronica.png"
