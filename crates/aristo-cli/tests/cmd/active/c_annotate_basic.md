# `aristo stamp --check` — C annotations discovered and in sync

Source: C annotate design (`docs/multilang-c-go-design-study.md`), slice C-1.

C has no attribute syntax, so annotations ride in `// @aristo intent(...)` /
`// @aristo assume(...)` line-comment directives placed directly above the
function they describe. The parenthesized argument list uses the same grammar
as the Rust `#[aristo::intent(...)]` macro. The source walk covers `.c`/`.h`
files alongside `.rs`, so the CI gate `aristo stamp --check` finds the two
intents in `db.c` and confirms the committed index is in sync with source.

```console
$ aristo stamp --check
→ Walking source from [..] …
→ Found 2 annotations
→ Checking for parent-link cycles
  status (from .aristo/proofs/): fresh: 0, stale: 0, refuted: 0, unverified: 2

ok: index is up to date (no rewrite needed).

```
