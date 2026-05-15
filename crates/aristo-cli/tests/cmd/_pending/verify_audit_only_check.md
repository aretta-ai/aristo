# `aristo verify --audit-only --check` — CI gate variant

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J1.c → CI gate variant".

`--check` composes with `--audit-only` to non-zero-exit on any critical finding (forged or orphan classifications). Used in downstream CI to gate on the integrity of upstream bindings.

```console
$ aristo verify --audit-only --check
? 2
[..]
error: 1 forged verified_outcome — refusing to gate verification
```
