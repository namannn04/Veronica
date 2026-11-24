#!/usr/bin/env bash
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

fetch() {
  curl --fail --silent --show-error --location \
    --proto '=https' --proto-redir '=https' \
    --connect-timeout 15 --max-time 300 \
    --retry 4 --retry-all-errors --retry-delay 1 --retry-max-time 600 "$@"
}

download_file() {
  local path="$1" url="$2" attempt code response remaining
  local deadline=$((SECONDS + 900))
  for attempt in 1 2 3 4 5; do
    remaining=$((deadline - SECONDS))
    [ "$remaining" -gt 0 ] || break
    [ "$remaining" -le 300 ] || remaining=300
    if response="$(fetch --retry 0 --max-time "$remaining" --continue-at - \
      --output "$path" --write-out '%{http_code}' "$url")"; then
      return 0
    else
      code=$?
    fi
    if [ "$code" -eq 33 ] || [ "$code" -eq 36 ] || [ "$response" = 416 ]; then
      rm -f -- "$path"
    fi
    if [ "$attempt" -lt 5 ] && [ "$SECONDS" -lt "$deadline" ]; then
      echo "Download interrupted; retrying (${attempt}/5)..." >&2
      sleep 1
    fi
  done
  return 1
}

app_version_pattern='^[0-9]+\.[0-9]+\.[0-9]+$'

newest_app_version() {
  fetch --header 'Accept: application/vnd.github+json' \
    "https://api.github.com/repos/${repository}/releases?per_page=100" |
    grep -oE '"tag_name": *"v[0-9]+\.[0-9]+\.[0-9]+"' |
    sed -E 's/.*"v([0-9.]+)"/\1/' |
    sort -t. -k1,1n -k2,2n -k3,3n | tail -n 1
}

version="${VERONICA_VERSION:-}"
if [ -z "$version" ]; then
  latest_url="$(fetch \
    --output /dev/null --write-out '%{url_effective}' "${release_root}/latest")"
  latest_tag="${latest_url#*/releases/tag/}"
  if [[ "$latest_tag" == v* ]] && [[ "${latest_tag#v}" =~ $app_version_pattern ]]; then
    version="${latest_tag#v}"
  else
    version="$(newest_app_version)" || fail "cannot find the newest Veronica release."
  fi
fi

[[ "$version" =~ $app_version_pattern ]] || fail "invalid release version: $version"

asset="Veronica_${version}_amd64.deb"
download_root="${release_root}/download/v${version}"
work_dir="$(mktemp -d)"
trap 'rm -rf -- "$work_dir"' EXIT

echo "Downloading Veronica ${version}..."
download_file "${work_dir}/${asset}" "${download_root}/${asset}"
download_file "${work_dir}/SHA256SUMS" "${download_root}/SHA256SUMS"

expected="$(awk -v asset="$asset" '$2 == asset { print $1 }' "${work_dir}/SHA256SUMS")"
[ -n "$expected" ] || fail "the release checksum does not list ${asset}."
printf '%s  %s\n' "$expected" "${work_dir}/${asset}" | sha256sum --check --status ||
  fail "checksum verification failed; nothing was installed."

echo "Checksum verified. Installing ${asset}..."
if [ "$(id -u)" -eq 0 ]; then
  apt install --reinstall --yes "${work_dir}/${asset}"
else
  command -v sudo >/dev/null 2>&1 || fail "sudo is required to install the Debian package."
  sudo apt install --reinstall --yes "${work_dir}/${asset}"
fi

uuid="veronica@namannn04.github.io"

enable_notch() {
  [ "$(id -u)" -ne 0 ] || return 1
  command -v gsettings >/dev/null 2>&1 || return 1
  gnome-extensions enable "$uuid" >/dev/null 2>&1 && return 0
  local current
  current="$(gsettings get org.gnome.shell enabled-extensions 2>/dev/null)" || return 1
  case "$current" in
    *"'${uuid}'"*) return 0 ;;
    "@as []" | "[]") current="['${uuid}']" ;;
    *) current="${current%]}, '${uuid}']" ;;
  esac
  gsettings set org.gnome.shell enabled-extensions "$current" 2>/dev/null
}

notch_active() {
  gnome-extensions info "$uuid" 2>/dev/null | grep -q "State: ACTIVE"
}

echo
echo "Veronica ${version} is installed."
if notch_active; then
  echo "The notch is already running; log out and back in to load this version."
elif enable_notch; then
  echo "The top-bar notch is enabled. Log out and back in once to see it."
else
  echo "To get the top-bar notch: log out and back in, then open Veronica → Settings → Enable."
fi

if [ "$(id -u)" -ne 0 ] && [ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ] &&
  ! pgrep -x veronica >/dev/null 2>&1; then
  setsid veronica >/dev/null 2>&1 </dev/null &
  echo "Opening Veronica…"
else
  echo "Open Veronica from Activities, or run: veronica"
fi
