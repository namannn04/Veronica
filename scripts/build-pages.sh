#!/usr/bin/env bash
# Assemble the static GitHub Pages artifact without duplicating install.sh.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
destination="${1:-${repo_root}/target/pages}"

case "$destination" in
  "" | / | "$repo_root")
    echo "Refusing unsafe Pages output directory: $destination" >&2
    exit 1
    ;;
esac

mkdir -p "$destination"
cp -R "$repo_root/site/." "$destination/"
cp "$repo_root/install.sh" "$destination/install"
chmod 0644 "$destination/install"

test -s "$destination/index.html"
test -s "$destination/styles.css"
test -s "$destination/app.js"
cmp --silent "$repo_root/install.sh" "$destination/install"
bash -n "$destination/install"

echo "GitHub Pages artifact ready at $destination"
