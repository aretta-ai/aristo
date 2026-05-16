# `aristo verify --audit-only --strict` — publisher-provenance cross-check

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J1.c → Strict mode".

`--strict` adds a network round-trip to `aretta.dev/registry/` to map each `linked = "arta_..."` to its registered publisher identity (crates.io account, git remote, Aristo org), then compares against the local repo's git remote. Mismatches are reported as soft signals (legitimate forks are normal) and exit code stays 0. Parallels `aristo badge --strict`.

```console
$ aristo verify --audit-only --strict
[..]
→ Cross-checking aretta.dev/registry/ for publisher identity … ([..] requests)
  • Registry lookup: arta_op4q3z9NbV → publisher: github.com/priyacorp/distrib-lock
  • Local git remote: github.com/priyacorp/distrib-lock-fork  ←  MISMATCH

warning: 1 publisher-identity mismatch (legitimate forks are normal — treat as soft signal)

```
