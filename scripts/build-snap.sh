#!/usr/bin/env bash
set -euo pipefail

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
deb="$(realpath "${1:?usage: $0 path/to/Veronica_<version>_amd64.deb}")"
version="$(dpkg-deb -f "$deb" Version)"
work="$repository/target/snap"
snap="$work/Veronica_${version}_amd64.snap"

rm -rf -- "$work"
mkdir -p "$work/snap"
sed "s/^version: .*/version: \"${version}\"/" "$repository/packaging/snap/snapcraft.yaml" >"$work/snap/snapcraft.yaml"
cp "$repository/packaging/linux/io.github.namannn04.Veronica.png" "$work/veronica.png"
cp "$deb" "$work/veronica.deb"
(cd "$work" && snapcraft pack --destructive-mode --output "$snap")
echo "built $snap"
