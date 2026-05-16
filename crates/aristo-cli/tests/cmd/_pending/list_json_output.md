# `aristo list --json` — structured output for tooling

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J1.a → Structured output".

JSON array of annotation records. Stale entries include a `stale_reason` discriminator (`text_hash` vs `body_hash`) for downstream tooling.

```console
$ aristo list --filter status=stale --json
[
  {
    "id": "aristos:edit_page_writes_each_cell_once",
    "kind": "intent",
    "verify": "full",
    "status": "stale",
    "file": "core/storage/btree.rs",
    "stale_reason": "body_hash"
  }
]

```
