# `aristo stamp` — cycle detection diagnostics

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F2.

Three cases: a multi-node cycle is reported with its full path + per-node sites; a self-cycle is rejected as a special case; a diamond pattern is allowed (it's a DAG, not a cycle, under AND-semantics for `parent`).

## Multi-node cycle: a → b → c → a

```console
$ aristo stamp
? 2

error: cycle detected in parent graph
       a → b → c → a

Break the cycle by removing one of these parent links:
  • a (src/lib.rs:[..])    has parent = "c"
  • b (src/lib.rs:[..])    has parent = "a"
  • c (src/lib.rs:[..])    has parent = "b"

No files modified. Fix the cycle and re-run `aristo stamp`.

```

## Self-cycle (F2-b)

```console
$ aristo stamp
? 2
error: self-cycle: annotation `self_loop` lists itself as parent
       (src/lib.rs:[..])
       An annotation cannot be its own parent.

```

## Diamond is a DAG, not a cycle (F2-c)

```console
$ aristo stamp
ok: 4 annotations stamped, no cycles detected.

```
