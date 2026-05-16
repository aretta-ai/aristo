# B5b state: `forged (tampered)` — signature does not verify against any bundled key

Source: `../aretta-sdk/docs/mockups/09-signature-scheme/cli-sessions.md` § "State 4: forged — signature does not verify".

A `verified_outcome` whose bytes don't decode to a valid Ed25519 signature, or one signed by a key not in the SDK's bundled registry. Either tampered after issuance or attacker-minted. CRITICAL diagnostic; treated as a security signal.

## `aristo stamp` surfaces the tampered outcome

```console
$ aristo stamp

CRITICAL: 1 forged verified outcome — DO NOT TRUST
  • aristos:page_type_discriminants_are_format_stable
        (core/storage/sqlite3_ondisk.rs:[..])
    verified_outcome bytes do not verify against any bundled Aristo public key.

    Classification: forged (tampered)
      The signature bytes don't match any known public key. Most likely
      the verified_outcome was modified after the server issued it.

    Aristo will treat this annotation as UNVERIFIED.
    To resolve:
      • Run `aristo verify --filter id=aristos:page_type_discriminants_are_format_stable`
        to obtain a fresh, valid outcome.
      • If you didn't intentionally edit the index, treat this as a security
        signal — someone may have edited .aristo/index.toml in your tree.
        Check `git log -- .aristo/index.toml`.

    Status: forged

```

## `aristo show` for a forged entry

```console
$ aristo show aristos:page_type_discriminants_are_format_stable

aristos:page_type_discriminants_are_format_stable  (intent)
  status:    forged  ✗  CRITICAL
  verify:    "full"
  file:      core/storage/sqlite3_ondisk.rs:[..]
  site:      impl PageType

  ✗  Forged (tampered) verified outcome:
     Ed25519 verification failed against every bundled public key.
     The signature bytes appear to have been modified.

     This is a security signal. Investigate `git log -- .aristo/index.toml`
     for unexpected changes, then run `aristo verify --filter id=aristos:page_type_discriminants_are_format_stable`
     to obtain a fresh outcome.

```
