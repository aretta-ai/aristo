---
date: 2026-05-15
slice: 18
file: crates/aristo-cli/src/commands/list.rs:47
id: matches_filter_arms_cover_filter_enum
verdict: delete
principles: [P-CHECK-TYPE-SYSTEM-FIRST, P-INVARIANT-NOT-IMPL]
verify_was: test
verify_is: (deleted)
---

## Original (v0)

> matches_filter ANDs filters at the call site; here it tests one clause against one entry. The set of supported clauses is closed by the Filter enum (id / file / parent / status) — adding a new filter key in J2 means extending Filter AND adding an arm here. A missing arm would silently never match, hiding entries from `aristo list --filter <new-key>=...` output.

## Better

(none — annotation deleted from source)

## Why deleted

The intent's central claim ("a missing arm would silently never match") is **factually wrong about Rust**. `match` on a closed enum is exhaustive — the compiler refuses to compile a missing arm. Adding a new `Filter` variant without extending `matches_filter` is a compile error, not a silent miss.

The whole property this intent tries to enforce is already given by the type system + exhaustive matching (P-CHECK-TYPE-SYSTEM-FIRST). Worse, the intent misframes the failure mode — the author was reasoning about it as if it were a string-based dispatch where a typo would silently fail, but the actual code is enum-exhaustive.

This is the most embarrassing case in the round: the annotation passes a casual read because the prose sounds plausible, but it doesn't survive a moment's interaction with how Rust actually works. It would teach the skill the wrong shape if held up as a dogfood example.

**Author-self-flag:** I wrote this in slice 18 with the original criterion in mind. The fact that an annotation can be wrong-and-confident is the strongest argument for the reflection loop itself.

## Verify level

n/a (deleted).
