# Vendored resources

## `refresh-usage`

The agent-usage collector, vendored from
[Edith](https://github.com/pulkitxm/edith)
(`Packages/Edith/Sources/EdithKit/Resources/refresh-usage`).

Edith is licensed GPL-3.0 and so is Veronica, so redistribution here is under the
same terms. It is kept byte-identical to upstream apart from the single fix
below, so that both projects report the same numbers from the same machine and
so that pulling a newer upstream version stays a small diff.

### The one change

On Linux the script aborted at `comm -23` with "input is not in sorted order".
Two files feeding `comm` were sorted differently: one came from `jq`'s `unique`,
which orders by codepoint, and both were then re-sorted by the shell under the
user's UTF-8 collation, while `comm` itself compares bytes. Forcing `LC_ALL=C` on
both sorts makes all three orderings agree:

```sh
jq -r '[.[].cwd] | unique | .[] | select(. != "")' "$TMP/recs.json" \
  | LC_ALL=C sort >"$TMP/cwds.txt"
jq -r '.cwd' "$TMP/cwdmap.jsonl" | LC_ALL=C sort -u >"$TMP/cwds-have.txt"
```

This never surfaced on macOS because its default collation happens to agree with
byte order for these paths.

### When updating from upstream

Reapply that fix. A test in `crates/veronica-usage` asserts both `LC_ALL=C`
sorts are present in the vendored copy, because without them collection fails at
its final stage with a confusing error.

The script is compiled into the binary with `include_str!` and written to the
cache directory on each launch, so an upgraded Veronica never runs a stale copy.
