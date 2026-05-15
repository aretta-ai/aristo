# B5b state: `forged (key revoked)` — signed by an SDK-revoked key

Source: `../aretta-sdk/docs/mockups/09-signature-scheme/cli-sessions.md` § "State 4: Forged variant — known but revoked key".

If a `verified_outcome` DOES verify against a key in the bundled registry, but that key has been revoked (entry in the SDK's revocation list), the classification is `forged (key revoked)` — distinguishable from `forged (tampered)` so users know to upgrade the SDK and re-verify, not suspect tampering.

```console
$ aristo stamp

CRITICAL: 1 forged verified outcome — DO NOT TRUST
  • aristos:some_old_annotation   (src/legacy.rs:[..])
    verified_outcome was signed by Aristo key scheme_version=[..] (rev_[..]).

    Classification: forged (key revoked)
      This key was retired in SDK release v[..] (revocation reason:
      precautionary rotation). Outcomes signed with this key are no longer
      trusted.

    To resolve:
      • Upgrade SDK to >= v[..] (you may already be on it; the revocation
        is what's flagging this).
      • Re-verify the annotation: aristo verify --filter id=aristos:some_old_annotation
        — will issue a fresh outcome under the current key.

    Status: forged
```
