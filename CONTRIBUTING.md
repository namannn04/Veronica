# Contributing to Veronica

Thank you for helping improve Veronica. Bug reports, design feedback,
documentation, tests, and code are welcome.

## Before You Start

- Search existing issues and pull requests before opening a duplicate.
- For a substantial feature, a packaging change, or architectural work, open an
  issue first so the approach can be discussed.
- Keep changes focused. Separate unrelated fixes into separate pull requests.
- Never include credentials, agent history, personal data, or build artifacts.

## Development Setup

Veronica targets Ubuntu 24.04 or later with GNOME Shell 46 through 50. It needs
Rust, Bun, and the Tauri system dependencies:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
  libayatana-appindicator3-dev libjavascriptcoregtk-4.1-dev \
  libsoup-3.0-dev libxdo-dev patchelf jq

git clone --recurse-submodules https://github.com/namannn04/Veronica.git
cd Veronica/apps/desktop
bun install
bunx tauri dev
```

[docs/BUILDING.md](docs/BUILDING.md) covers the toolchain, the shell extension,
and packaging in detail, and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
explains the crate layout and the capability model.

## Required Checks

CI runs these on every pull request. Run them locally before you push:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
(cd extension && npm test)
(cd apps/desktop && bun test && bun run build)
```

CI also tests the Graftcode receiver, the agent status hook, and the companion
runtime, then builds the Debian package and smoke-tests the packaged `vr`.

## Guidelines

- Veronica controls the user's machine. Keep destructive or privileged
  operations explicit, confirmed, and reviewable before they run.
- Pass arguments to subprocesses directly; never build shell strings from user
  or file data.
- Keep agent history and other private data on the machine. A feature that
  needs the network must say so and run only when the user asks for it.
- Put behavior in the Rust crates and keep the interface and the shell
  extension as callers of it, so the app and `vr` stay in agreement.
- Add focused tests for behavior changes.
- Update `README.md` and `docs/` when user-facing behavior changes.

## Pull Requests

A pull request description is one line that states what changed and why. Use
conventional commit subjects, keep the diff focused, and add tests for changed
behavior. When the change has something to show, post a screenshot, recording,
or clean terminal output of the finished result as a comment.

Maintainers may request changes before merge. By contributing, you agree that
your contribution is licensed under the GNU General Public License version 3
with the additional terms in [NOTICE](NOTICE).

## Reporting Security Issues

Do not publish exploitable security issues in a public issue. Follow
[SECURITY.md](SECURITY.md) and use GitHub's private vulnerability reporting
form instead.

## Releases

Maintainers cut releases. A pushed `v<version>` tag builds the Debian package,
AppImage, Flatpak, and Snap, publishes them to a GitHub release, and refreshes
the apt and Flatpak repositories. Contributors should not bump the version.
