# Desktop integration tests

## Extension frame isolation

`frame_isolation.rs` starts the real application with the hostile bundle in
`fixtures/extensions/hostile` loaded in an extension frame. The bundle tries to
reach the host in every way it can and reports each attempt over its message
port. The test fails if any attempt succeeds, or if the bundle's own script,
stylesheet or font fails to load.

The test needs a display, a private session bus and the built interface, so it
is ignored by default. Run it from the repository root:

```sh
(cd apps/desktop && bun install --frozen-lockfile && bun run build)
dbus-run-session -- cargo test -p veronica-desktop --locked \
  --features tauri/custom-protocol --test frame_isolation -- --ignored --nocapture
```

On a machine without a display, wrap the second command in `xvfb-run -a`:

```sh
xvfb-run -a dbus-run-session -- cargo test -p veronica-desktop --locked \
  --features tauri/custom-protocol --test frame_isolation -- --ignored --nocapture
```

The application runs against a temporary home directory, so it never touches
your settings. `--nocapture` prints the verdict for every attempt.

The two environment variables the test sets only work in debug builds:

- `VERONICA_EXTENSIONS_DEV` names a directory whose subdirectories are served
  as `veronica-ext://<name>/`.
- `VERONICA_FRAME_PROBE` names the file the report is written to, and opens the
  probe surface instead of the home page.
