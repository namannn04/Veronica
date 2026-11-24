#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

vr_pack="${VR_PACK:-vr-pack}"
base_ref="${PLAN_BASE:-}"
excluded=" ${PLAN_EXCLUDE-fixture} "

"$vr_pack" validate >&2

base=""
if [ -n "$base_ref" ]; then
  base="$(git merge-base "$base_ref" HEAD)"
fi

entries=()
for manifest in extensions/*/manifest.json; do
  [ -f "$manifest" ] || continue
  dir="${manifest%/manifest.json}"
  id="${dir#extensions/}"
  case "$excluded" in
    *" $id "*) continue ;;
  esac
  report="$("$vr_pack" --json fingerprint "$id")"
  version="$("$vr_pack" version "$id")"
  if [ -n "$base" ]; then
    mapfile -t inputs < <(jq -r '.inputs[]' <<<"$report")
    if git diff --quiet "$base" HEAD -- "${inputs[@]}"; then
      echo "$id $version: unchanged since $base" >&2
      continue
    fi
  elif [ -z "${PLAN_ALL:-}" ] && git rev-parse -q --verify "refs/tags/ext/$id/v$version" >/dev/null; then
    echo "$id $version: already tagged" >&2
    continue
  fi
  echo "$id $version: build" >&2
  entries+=("$(jq -c --arg version "$version" '{id, version: $version, fingerprint}' <<<"$report")")
done

if [ "${#entries[@]}" -eq 0 ]; then
  echo "[]"
else
  printf '%s\n' "${entries[@]}" | jq -cs .
fi
