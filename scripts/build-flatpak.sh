#!/usr/bin/env bash
# Build the Flatpak bundle from a Debian package the release build produced.
#
# Usage: scripts/build-flatpak.sh path/to/Veronica_<version>_amd64.deb
#
# Writes target/flatpak/Veronica_<version>_x86_64.flatpak. The GNOME 50 SDK and
# runtime come from Flathub, so the first build downloads them.
set -euo pipefail

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
deb="$(realpath "${1:?usage: $0 path/to/Veronica_<version>_amd64.deb}")"
version="$(jq -r .version "$repository/apps/desktop/src-tauri/tauri.conf.json")"
manifest="$repository/packaging/flatpak/io.github.namannn04.Veronica.yml"
work="$repository/target/flatpak"
bundle="$work/Veronica_${version}_x86_64.flatpak"

if [ ! -f "$repository/packaging/flatpak/shared-modules/libayatana-appindicator/libayatana-appindicator-gtk3.json" ]; then
  echo "the shared-modules submodule is missing; run: git submodule update --init" >&2
  exit 1
fi

# Prefer a native flatpak-builder (CI installs one); the Flathub app works too.
if command -v flatpak-builder >/dev/null 2>&1; then
  builder=(flatpak-builder)
else
  builder=(flatpak run --filesystem="$repository" org.flatpak.Builder)
fi

flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
mkdir -p "$work"
cp "$deb" "$work/veronica.deb"
"${builder[@]}" --user --force-clean --install-deps-from=flathub --disable-rofiles-fuse \
  --state-dir="$work/state" --repo="$work/repo" --default-branch=stable \
  "$work/build" "$manifest"
flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
  "$work/repo" "$bundle" io.github.namannn04.Veronica stable
echo "built $bundle"
