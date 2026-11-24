#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
exec uv run --quiet --no-project --python 3.12 \
  --with "tree-sitter==0.23.2" \
  --with "tree-sitter-language-pack==0.6.1" \
  python scripts/comments.py "$@"
