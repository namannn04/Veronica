#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

shell_files() { git ls-files -z '*.sh' install.sh resources/refresh-usage | sort -zu; }

em_dash() {
  if git grep -nI $'—' -- ':!packaging/flatpak/shared-modules'; then
    echo "em-dash characters are not allowed; rewrite with a comma, colon or a new sentence" >&2
    return 1
  fi
}

comments() { scripts/comments.sh; }

typos_check() { uvx --quiet typos==1.40.0; }

rust() {
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets --locked -- -D warnings
}

web() {
  bunx --bun @biomejs/biome@2.6.6 ci .
  (cd apps/desktop && bunx tsc --noEmit)
}

python_check() {
  uvx --quiet ruff@0.14.0 check .
  uvx --quiet ruff@0.14.0 format --check .
}

shell_check() {
  shell_files | xargs -0 uvx --quiet --from shellcheck-py==0.11.0.1 shellcheck -S style
  shell_files | xargs -0 uvx --quiet --from shfmt-py==3.12.0.2 shfmt -d -i 2 -ci
}

yaml_check() { uvx --quiet yamllint==1.37.1 --strict .; }

markdown_check() { bunx --bun markdownlint-cli2@0.20.0 "**/*.md"; }

workflows_check() {
  uvx --quiet --from actionlint-py==1.7.8.24 actionlint
  uvx --quiet zizmor@1.18.0 --persona=pedantic --min-severity=low .github/workflows
}

all_checks=(em_dash comments typos_check rust web python_check shell_check yaml_check markdown_check workflows_check)

if [ "$#" -eq 0 ]; then
  set -- "${all_checks[@]}"
fi

failed=()
for check in "$@"; do
  echo "::group::$check"
  if ! "$check"; then
    failed+=("$check")
  fi
  echo "::endgroup::"
done

if [ "${#failed[@]}" -gt 0 ]; then
  echo "failed: ${failed[*]}" >&2
  exit 1
fi
