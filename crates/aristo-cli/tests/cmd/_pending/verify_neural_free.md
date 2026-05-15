# `aristo verify` — `verify = "neural"` on free tier (local skill only)

Source: `../aretta-sdk/docs/diagrams/03-verify-execution.mmd` § `n_tier=Free → n_free → out_status` ("aristo-neural-verify skill via Cursor / Claude Code").

The free-tier path for `verify = "neural"` is purely local: the host coding agent (Cursor / Claude Code / etc.) runs the `aristo-neural-verify` skill against the annotation's text and the surrounding source, and produces a status verdict. No mining, no spec write, no `cargo test` invocation, no signed `verified_outcome` — just a status update on the index entry. Contrast with `verify = "test"` (which writes a spec + runs cargo test) and `verify = "neural"` paid (which produces a signed outcome via the HQ neural model).

Output observable: `out_status` in the diagram — index entry's `status` field updates; nothing else changes.

## Default run

```console
$ aristo verify

→ Running verification (free tier; local skills only) …

→ Invoking aristo-neural-verify skill via [..] … [..]
  • api_idempotency           neural verdict: holds      (confidence: high)
  • cache_consistency         neural verdict: holds      (confidence: medium)
  • retry_safe_on_5xx         neural verdict: violated   (counterexample sketched in skill output)

ok: 3 annotations verified (method: neural).
  • api_idempotency           status: verified
  • cache_consistency         status: verified
  • retry_safe_on_5xx         status: failed
```

## Filter to a single id (composes with J2 `--filter`)

```console
$ aristo verify --filter id=api_idempotency

→ Running verification (free tier; local skills only) …
→ Invoking aristo-neural-verify skill via [..] … [..]
  • api_idempotency           neural verdict: holds      (confidence: high)

ok: 1 annotation verified (method: neural).
  • api_idempotency           status: verified
```

## Index reflects the status, no `verified_outcome` field

```console
$ aristo show api_idempotency
api_idempotency (intent)
  status:    verified
  method:    neural
  binding:   local
  text_hash: sha256:[..]
  body_hash: sha256:[..]
[..]
```
