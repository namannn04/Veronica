#!/usr/bin/env bash
# Install the latest verified Veronica Debian release on Ubuntu x86_64.
set -euo pipefail

repository="namannn04/Veronica"
release_root="https://github.com/${repository}/releases"

fail() {
  echo "Veronica installer: $*" >&2
  exit 1
}

[ "$(uname -s)" = "Linux" ] || fail "Linux is required."
case "$(uname -m)" in
  x86_64 | amd64) ;;
  *) fail "only x86_64/amd64 is currently supported." ;;
esac

[ -r /etc/os-release ] || fail "cannot identify this Linux distribution."
os_id="$(awk -F= '$1 == "ID" { gsub(/\"/, "", $2); print $2 }' /etc/os-release)"
os_version="$(awk -F= '$1 == "VERSION_ID" { gsub(/\"/, "", $2); print $2 }' /etc/os-release)"
[ "$os_id" = "ubuntu" ] || fail "the Debian package currently supports Ubuntu only."
os_major="${os_version%%.*}"
[ "$os_major" -ge 24 ] 2>/dev/null || fail "Ubuntu 24.04 or later is required."

for command in curl sha256sum; do
  command -v "$command" >/dev/null 2>&1 || fail "$command is required."
done
command -v apt >/dev/null 2>&1 || fail "apt is required; use the AppImage on non-Debian systems."

version="${VERONICA_VERSION:-}"
if [ -z "$version" ]; then
  latest_url="$(curl --fail --silent --show-error --location \
    --output /dev/null --write-out '%{url_effective}' "${release_root}/latest")"
  version="${latest_url##*/}"
  version="${version#v}"
fi

[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "invalid release version: $version"

asset="Veronica_${version}_amd64.deb"
download_root="${release_root}/download/v${version}"
work_dir="$(mktemp -d)"
trap 'rm -rf -- "$work_dir"' EXIT

echo "Downloading Veronica ${version}..."
curl --fail --silent --show-error --location \
  --output "${work_dir}/${asset}" "${download_root}/${asset}"
curl --fail --silent --show-error --location \
  --output "${work_dir}/SHA256SUMS" "${download_root}/SHA256SUMS"

expected="$(awk -v asset="$asset" '$2 == asset { print $1 }' "${work_dir}/SHA256SUMS")"
[ -n "$expected" ] || fail "the release checksum does not list ${asset}."
printf '%s  %s\n' "$expected" "${work_dir}/${asset}" | sha256sum --check --status \
  || fail "checksum verification failed; nothing was installed."

echo "Checksum verified. Installing ${asset}..."
if [ "$(id -u)" -eq 0 ]; then
  apt install --reinstall --yes "${work_dir}/${asset}"
else
  command -v sudo >/dev/null 2>&1 || fail "sudo is required to install the Debian package."
  sudo apt install --reinstall --yes "${work_dir}/${asset}"
fi

echo "Veronica ${version} is installed. Open Veronica from Activities."
