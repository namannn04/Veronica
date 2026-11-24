#!/usr/bin/env bash
set -euo pipefail

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
usage="usage: $0 deb-directory destination gpg-key-id"
debs="$(realpath "${1:?$usage}")"
destination="${2:?$usage}"
key_id="${3:?$usage}"

output="$destination/apt"
rm -rf -- "$output"
pool="$output/pool/main/v/veronica"
index="$output/dists/stable/main/binary-amd64"
mkdir -p "$pool" "$index"

shopt -s nullglob
packages=("$debs"/*.deb)
[ "${#packages[@]}" -gt 0 ] || { echo "no .deb files in $debs" >&2; exit 1; }
for package in "${packages[@]}"; do
  name="$(dpkg-deb -f "$package" Package)"
  version="$(dpkg-deb -f "$package" Version)"
  architecture="$(dpkg-deb -f "$package" Architecture)"
  cp "$package" "$pool/${name}_${version}_${architecture}.deb"
done

(cd "$output" && apt-ftparchive packages pool) > "$index/Packages"
gzip -9nk "$index/Packages"
apt-ftparchive \
  -o APT::FTPArchive::Release::Origin=Veronica \
  -o APT::FTPArchive::Release::Label=Veronica \
  -o APT::FTPArchive::Release::Suite=stable \
  -o APT::FTPArchive::Release::Codename=stable \
  -o APT::FTPArchive::Release::Architectures=amd64 \
  -o APT::FTPArchive::Release::Components=main \
  -o APT::FTPArchive::Release::Description="Veronica, the native control center for Ubuntu" \
  release "$output/dists/stable" > "$output/Release.tmp"
mv "$output/Release.tmp" "$output/dists/stable/Release"
gpg --batch --yes --local-user "$key_id" --clearsign \
  --output "$output/dists/stable/InRelease" "$output/dists/stable/Release"
gpg --batch --yes --local-user "$key_id" --armor --detach-sign \
  --output "$output/dists/stable/Release.gpg" "$output/dists/stable/Release"

cp "$repository/packaging/flatpak/veronica-repo.gpg" "$output/veronica.gpg"
gpg --batch --yes --enarmor --output - "$repository/packaging/flatpak/veronica-repo.gpg" \
  | sed -e 's/ARMORED FILE/PUBLIC KEY BLOCK/' -e '/^Comment:/d' > "$output/veronica.asc"

echo "apt repository ready at $output"
