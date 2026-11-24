# Repository workflow

Follow the always-applicable GitHub and release workflow in [.cursor/rules/github-release-workflow.mdc](.cursor/rules/github-release-workflow.mdc).

## Code rules

CI enforces every rule below, and `main` only accepts a pull request when the `ci-gate` check passes. Run `scripts/check.sh` before pushing; `scripts/check.sh rust web` runs a subset.

- No em-dash characters anywhere in the repository. Use a comma, a colon or a new sentence.
- No code comments in any language. Names and structure carry the meaning. Only tool directives survive (`@ts-`, `biome-ignore`, `shellcheck`, `noqa`, shebangs, `/*!` license blocks). `scripts/comments.sh --fix` removes the rest.
- Rust follows the workspace lint policy in `Cargo.toml` (Clippy pedantic as errors, `#[expect(lint, reason = "...")]` instead of `#[allow]`), `rustfmt`, `cargo-deny` and `cargo-machete`, on the toolchain pinned in `rust-toolchain.toml`.
- TypeScript, JavaScript and CSS follow `biome.json` (format and lint) and strict `tsc`.
- Python follows `ruff.toml`; shell scripts pass ShellCheck and `shfmt -i 2 -ci`; YAML passes yamllint; Markdown passes markdownlint; workflows pass actionlint and zizmor, with every action pinned to a commit SHA.
- Pull requests are squash merged.
