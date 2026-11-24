#!/usr/bin/env bash
set -euo pipefail

usage="usage: $0 output-directory [series...]"
output="$(mkdir -p "${1:?$usage}" && realpath "$1")"
shift
series=("$@")
[ "${#series[@]}" -gt 0 ] || series=(noble resolute)
maintainer="${PPA_MAINTAINER:-Naman Dadhich <namannn04@users.noreply.github.com>}"

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
config="$repository/apps/desktop/src-tauri/tauri.conf.json"
version="$(jq -r .version "$config")"

for command in cargo cargo-vendor-filterer dpkg-buildpackage jq xz; do
  command -v "$command" >/dev/null 2>&1 || {
    echo "$command is required" >&2
    exit 1
  }
done
[ -s "$repository/apps/desktop/dist/index.html" ] || {
  echo "build the interface first: (cd apps/desktop && bun run build)" >&2
  exit 1
}
[ -s "$repository/packaging/bundled-extensions/index.json" ] || {
  echo "gather the bundled extensions first: cargo run -p veronica-pack -- bundle --packages target/ext --out packaging/bundled-extensions" >&2
  exit 1
}

work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT
source_dir="$work/veronica-$version"

git -C "$repository" archive --format=tar --prefix="veronica-$version/" HEAD | tar -x -C "$work"
cp -R "$repository/apps/desktop/dist" "$source_dir/apps/desktop/dist"
cp -R "$repository/packaging/bundled-extensions" "$source_dir/packaging/bundled-extensions"
(cd "$repository" && cargo vendor-filterer --platform=x86_64-unknown-linux-gnu "$source_dir/vendor" >/dev/null)
for checksum in "$source_dir"/vendor/*/.cargo-checksum.json; do
  jq -c '.files = {}' "$checksum" >"$checksum.new"
  mv "$checksum.new" "$checksum"
done
mkdir -p "$source_dir/.cargo"
cat >>"$source_dir/.cargo/config.toml" <<'CONFIG'

[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
CONFIG

tar -C "$work" --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner \
  -cf - "veronica-$version" | xz -9 -T0 >"$work/veronica_$version.orig.tar.xz"
cp "$work/veronica_$version.orig.tar.xz" "$output/"

depends="$(jq -r '.bundle.linux.deb.depends | join(", ")' "$config")"
recommends="$(jq -r '.bundle.linux.deb.recommends // [] | join(", ")' "$config")"
for name in "${series[@]}"; do
  rm -rf -- "$source_dir/debian"
  cp -R "$repository/packaging/ppa/debian" "$source_dir/debian"
  sed -i -e "s|@MAINTAINER@|$maintainer|" -e "s|@RUNTIME_DEPENDS@|$depends|" \
    -e "s|@RUNTIME_RECOMMENDS@|$recommends|" "$source_dir/debian/control"
  cat >"$source_dir/debian/changelog" <<CHANGELOG
veronica ($version-1~${name}1) $name; urgency=medium

  * Veronica $version.

 -- $maintainer  $(date -R)
CHANGELOG
  (cd "$source_dir" && dpkg-buildpackage --build=source -d -nc -sa -us -uc </dev/null)
  find "$work" -maxdepth 1 -type f \( -name '*.dsc' -o -name '*.debian.tar.xz' \
    -o -name '*_source.changes' -o -name '*_source.buildinfo' \) -exec mv {} "$output/" \;
done
ls -l "$output"
