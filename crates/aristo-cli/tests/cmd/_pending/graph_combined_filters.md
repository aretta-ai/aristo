# `aristo graph` — composed filters

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → Combined filters".

Any combination of `--filter`, `--depth`, `--exclude-assumes`, `--include-status`, `--include-orphans`, `--format`, and `--out` composes. Multiple `--filter` flags AND together (per the J2 unified grammar).

```console
$ aristo graph --filter file=core/storage/btree.rs --exclude-assumes --depth=2 --format=svg --out=btree-intents.svg

→ Reading .aristo/index.toml … ok
→ Filtering: btree.rs + intents only + depth 2 from filter root
→ Rendering DOT graph in memory …
→ Shelling out to `dot -Tsvg` … ok
ok: wrote [..] nodes, [..] edges to btree-intents.svg
```
