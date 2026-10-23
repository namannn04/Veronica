#!/usr/bin/env bash
# Turn a Flatpak bundle into the signed repository GitHub Pages serves.
#
# Usage: scripts/build-flatpak-repo.sh Veronica.flatpak destination GPG_KEY_ID
#
# Writes destination/flatpak/repo plus the .flatpakref and .flatpakrepo files
# that `flatpak install` reads. The signing key must already be in the GnuPG
# keyring ($GNUPGHOME); its public half is packaging/flatpak/veronica-repo.gpg.
set -euo pipefail

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bundle="${1:?usage: $0 bundle.flatpak destination gpg-key-id}"
destination="${2:?usage: $0 bundle.flatpak destination gpg-key-id}"
key_id="${3:?usage: $0 bundle.flatpak destination gpg-key-id}"
base_url="https://namannn04.github.io/Veronica/flatpak"
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
Homepage=https://namannn04.github.io/Veronica/
GPGKey=$public_key
REF
cat > "$output/veronica.flatpakrepo" <<REPO
[Flatpak Repo]
Title=Veronica
Url=$base_url/repo/
Homepage=https://namannn04.github.io/Veronica/
Comment=Native control center for Ubuntu
GPGKey=$public_key
REPO

echo "Flatpak repository ready at $output"
