#!/usr/bin/env bash
# Build both Linux distributions, smoke-test the Debian payload and checksum it.
set -euo pipefail

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository"

# Desktop-launched terminals do not always inherit rustup's shell profile.
# Tauri invokes `cargo` by name, so make the standard rustup install visible
# before doing any release work and fail in words when Rust is truly absent.
if ! command -v cargo >/dev/null 2>&1; then
  rustup_bin="${CARGO_HOME:-$HOME/.cargo}/bin"
  if [ -x "$rustup_bin/cargo" ]; then
    export PATH="$rustup_bin:$PATH"
  else
    echo "cargo is required; install Rust with rustup before building a release" >&2
    exit 1
  fi
fi

if [ -n "$(git status --porcelain)" ]; then
  echo "release builds require a clean checkout" >&2
  exit 1
fi

version="$(jq -r .version apps/desktop/src-tauri/tauri.conf.json)"
cd apps/desktop
bunx tauri build --bundles deb,appimage --ci
cd "$repository"

deb="target/release/bundle/deb/Veronica_${version}_amd64.deb"
appimage="target/release/bundle/appimage/Veronica_${version}_amd64.AppImage"
for artifact in "$deb" "$appimage"; do
  if [ ! -f "$artifact" ]; then
    echo "missing release artifact: $artifact" >&2
    exit 1
  fi
done

audit_dir="$(mktemp -d)"
trap 'rm -rf -- "$audit_dir"' EXIT
dpkg-deb -x "$deb" "$audit_dir"
"$audit_dir/usr/bin/vr" extensions --help | grep -q enable
dpkg-deb -f "$deb" Depends | grep -q curl

checksums="target/release/bundle/SHA256SUMS"
(
  cd "$(dirname "$deb")"
  sha256sum "$(basename "$deb")"
  cd "../appimage"
  sha256sum "$(basename "$appimage")"
) > "$checksums"
echo "built Veronica $version"
echo "checksums: $checksums"
