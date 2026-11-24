#!/usr/bin/env bash
set -euo pipefail

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bundle="${1:?usage: $0 bundle.flatpak destination gpg-key-id}"
destination="${2:?usage: $0 bundle.flatpak destination gpg-key-id}"
key_id="${3:?usage: $0 bundle.flatpak destination gpg-key-id}"
base_url="https://namannn04.github.io/veronica/flatpak"
app_id="io.github.namannn04.Veronica"

output="$destination/flatpak"
rm -rf -- "$output"
mkdir -p "$output"
ostree init --mode=archive-z2 --repo="$output/repo"
flatpak build-import-bundle --gpg-sign="$key_id" "$output/repo" "$bundle"
flatpak build-update-repo --gpg-sign="$key_id" --generate-static-deltas \
  --title="Veronica" --default-branch=stable "$output/repo"

public_key="$(base64 --wrap=0 "$repository/packaging/flatpak/veronica-repo.gpg")"
cat > "$output/veronica.flatpakref" <<REF
[Flatpak Ref]
Name=$app_id
Branch=stable
Title=Veronica
Url=$base_url/repo/
SuggestRemoteName=veronica
RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo
IsRuntime=false
Homepage=https://namannn04.github.io/veronica/
GPGKey=$public_key
REF
cat > "$output/veronica.flatpakrepo" <<REPO
[Flatpak Repo]
Title=Veronica
Url=$base_url/repo/
Homepage=https://namannn04.github.io/veronica/
Comment=Native control center for Ubuntu
GPGKey=$public_key
REPO

echo "Flatpak repository ready at $output"
