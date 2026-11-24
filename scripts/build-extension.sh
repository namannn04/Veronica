#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

id="${1:?usage: $0 extension-id [output-directory]}"
out="${2:-target/ext}"
cargo_target="${CARGO_TARGET_DIR:-target}"
vr_pack="${VR_PACK:-$cargo_target/release/vr-pack}"
musl_target="${MUSL_TARGET:-x86_64-unknown-linux-musl}"
manifest="extensions/$id/manifest.json"

args=(--out "$out")
if jq -e 'has("backend")' "$manifest" >/dev/null; then
  cargo build --release --locked -p "veronica-ext-$id" --target "$musl_target"
  args+=(--backend "$cargo_target/$musl_target/release/veronica-ext-$id")
fi
if jq -e 'has("ui")' "$manifest" >/dev/null; then
  (cd "extensions/$id/ui" && bun install --frozen-lockfile && bun run build)
  args+=(--ui "extensions/$id/ui/dist")
fi

archive="$("$vr_pack" --json pack "$id" "${args[@]}" | jq -r .archive)"
"$vr_pack" audit "$archive"
echo "$archive"
