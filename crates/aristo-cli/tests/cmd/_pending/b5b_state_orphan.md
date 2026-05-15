# B5b state: `orphan` — signature real, but for a different repo's commit

Source: `../aretta-sdk/docs/mockups/09-signature-scheme/cli-sessions.md` § "State 3: orphan — signature real, but for a different repo's commit".

Most common cause: a developer copies a `.aristo/index.toml` entry from another repo (or someone else's crate) into theirs, hoping to claim verification. The signature is genuinely Aristo's — but for a `commit_hash` not in this repo's history. CRITICAL diagnostic; non-zero exit on `--check`.

## `aristo stamp` surfaces the orphan with CRITICAL diagnostic

```console
$ aristo stamp
ok: [..] annotations stamped, 0 ids assigned

CRITICAL: 1 orphan verified outcome — DO NOT TRUST
  • aristos:cell_array_indices_in_bounds   (core/storage/btree.rs:[..])
    verified_outcome signed_at:    [..]
    verified_outcome commit_hash:  [..]
                                   ↑ NOT in this repository's history.

    This binding's verification was issued for a different codebase.
    It was most likely copied from another repository's index.

    Aristo will treat this annotation as UNVERIFIED. To resolve:
      • If you own this code, run:  aristo verify --filter id=aristos:cell_array_indices_in_bounds
        (issues a fresh outcome bound to YOUR commit)
      • If this was copied unintentionally, run:  aristo unbind aristos:cell_array_indices_in_bounds
        (strips the namespace prefix and removes the index binding)
      • If you believe this is a mistake (e.g., your CI is doing strange things
        with shallow clones), see the shallow-clone note below.

    Status: orphan
```

## `aristo show` for an orphan

```console
$ aristo show aristos:cell_array_indices_in_bounds

aristos:cell_array_indices_in_bounds  (intent)
  status:    orphan  ✗  CRITICAL
  verify:    "test"
  file:      core/storage/btree.rs:[..]
  site:      fn balance_non_root

  ✗  Orphan verified outcome:
     The signed verified_outcome refers to commit [..], which
     is NOT in this repository's history. This binding does not belong here.

     Most likely cause: index entry was copied from another repo.
     Less likely: this commit was force-pushed away from your repo.

     Run `aristo verify --filter id=aristos:cell_array_indices_in_bounds` to
     issue a fresh outcome for current HEAD, or `aristo unbind
     aristos:cell_array_indices_in_bounds` to remove the binding.
```

## `aristo verify --check` exits non-zero (exit code 2)

```console
$ aristo verify --check
? 2
error: 1 orphan verified outcome — refusing to gate verification
  • aristos:cell_array_indices_in_bounds
    Commit [..] is not in this repository's history.
```
