# `aristo doc --include-status` — bake current B5b status into rendered docs

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I1 → `--include-status`".

Status (`verified` / `stale` / `orphan` / `forged`) is deliberately NOT in the default output — it's a build-time fact and would lie on a clean checkout. `--include-status` is the opt-in escape hatch for internal docs and CI snapshots, with a footer reminder that the embedded status will go stale.

```console
$ aristo doc --include-status

→ Reading .aristo/index.toml … ok ([..] annotations)
→ Including current B5b verification status in rendered docs.
→ Generating per-annotation markdown to .aristo/doc/ …
  ([..] files written, including status blocks)

ok: doc artifacts updated.

ℹ Status is a build-time fact and will become stale as code evolves.
   Re-run `aristo doc --include-status` to refresh, or omit `--include-status`
   for purely static rendering.
```
