# `aristo show <id>` — full record + reverse-walked children

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F3.

The id selector returns the annotation's full index entry (status, verify, file:line, hashes) followed by every annotation that lists it as a `parent` (the reverse-walk per F3-b). Slice 18 ships the offline subset — server-binding fields (`linked`, `verified_outcome`) are surfaced in the same block when present, but `aristo sync` (Phase 2) is needed to populate them; in the meantime, local-only entries omit those rows.

The sandbox pre-populates `src/lib.rs` (see the `.in/` fixture) with a single `#[aristo::intent]`. trycmd shells out commands directly without a real shell, so file-creation goes through the `.in/` mechanism rather than inline `printf`.

## Phase-1 output: bootstrap, stamp, show

```console
$ aristo init
ok: created aristo.toml
ok: created .aristo/index.toml (empty; 0 annotations)
ok: created .aristo/specs/
ok: created .aristo/doc/
ok: wrote .github/workflows/aristo.yml (starter; edit freely)

$ aristo stamp
→ Walking source from [..] …
→ Found 1 annotations
→ Building index entries
→ Detecting cycles in parent graph
  new: 1, unchanged: 0, body-drifted: 0, text-changed: 0, removed: 0

ok: stamped 1 annotations into .aristo/index.toml

$ aristo show returns_forty_two
returns_forty_two (intent)
  status:    unknown
  verify:    test
  file:      src/lib.rs
  site:      fn answer (line [..])
  covered_region: function
  text_hash: sha256:[..]
  body_hash: sha256:[..]

  Text:
    the answer is forty-two

```
