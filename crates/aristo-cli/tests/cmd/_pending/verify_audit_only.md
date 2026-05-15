# `aristo verify --audit-only` — offline B5b audit (J1.c)

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J1.c — `aristo verify --audit-only`".

Offline-validates every `verified_outcome` in `.aristo/index.toml` against bundled public keys; runs the B5b four-check pipeline (signature validity → identity → content hashes → commit ancestry); reports counts across the diagnostic states. Never modifies the index. Never requires auth — works for free-tier users auditing paid crates pulled from crates.io.

Supersedes the previously-proposed `aristo verify-bindings` (per J1).

```console
$ aristo verify --audit-only

→ Reading .aristo/index.toml … ok ([..] entries; [..] with aristos: namespace)
→ Validating bundled public key registry … ok (scheme v1, [..] active keys)
→ Auditing verified_outcome signatures offline …

  ✓ [..] verified
  ⚠  [..] stale            (sig valid; code has drifted since verification)
  ⚠  [..] pending-deepen   (outside shallow-clone window — informational)
  ✗ 0 orphan
  ✗ 0 forged

ok: bindings appear authentic. No mutations made.
```
