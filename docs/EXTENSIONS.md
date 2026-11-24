# Extensions

Extensions live under `extensions/<id>/`. CI validates every manifest, builds
the packages that changed, and on `main` publishes each new version as its own
GitHub release.

## Publishing, for maintainers

Releases and the signed catalog stay switched off until the repository owner
completes these steps once. Until then every publishing step skips with a
notice, so `main` stays green and nothing is published with the development
key.

1. Generate the catalog signing key on a trusted machine:

   ```bash
   cargo run --release -p veronica-pack -- keygen --out-private extensions-catalog.key
   ```

   It prints the public key and its key id. Keep the private key file offline
   as the backup.

2. In the repository settings, create an environment named
   `extension-catalog`, restrict its deployment branches to `main`, and add the
   secret `EXTENSIONS_SIGNING_KEY` holding the contents of
   `extensions-catalog.key`.

3. Replace the development key in `crates/veronica-host/src/trust.rs`: add a
   `TrustedKey` constant with the printed key id and public key, and put it in
   the `KEYS` slot that holds `DEVELOPMENT_KEY`. Re-sign
   `crates/veronica-host/seed/catalog.signed.json` with the new key in the same
   pull request. Clients only trust keys compiled into `trust.rs`, so this must
   ship in an app release before the first extension is published.

4. Turn on immutable releases for the repository.

`vr-pack key-status --key env:EXTENSIONS_SIGNING_KEY` reports whether a key is a
release key in `trust.rs`. Both workflows run it and publish nothing unless it
is.

## What the workflows do

- `extensions-publish.yml` runs after `ci-gate` on every push to `main`. Each
  newly built extension becomes the release `ext/<id>/v<version>` with
  `--latest=false`, so the app's `v*` releases stay the latest. A tag that
  already exists with the same fingerprint is skipped; a different fingerprint
  fails the run.
- `pages.yml` collects the metadata of every `ext/*` release, builds the
  catalog, signs it in the `extension-catalog` environment, verifies it against
  the public key compiled into `trust.rs`, and publishes it as
  `extensions/v1/catalog.signed.json`. A weekly run re-signs it so its 30-day
  expiry keeps moving.
- The installer, the website, the in-app update check and the package
  repositories only consider `vX.Y.Z` release tags, so an extension release is
  never mistaken for an app release.
