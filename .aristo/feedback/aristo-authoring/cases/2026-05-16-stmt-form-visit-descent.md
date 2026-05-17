---
date: 2026-05-16
slice: post-23
file: crates/aristo-core/src/walk/extract.rs (visit_stmt_macro)
id: stmt_form_intents_use_open_visit_descent_not_whitelist
verdict: new-intent-from-root-caused-bug
principles: [P-ROOT-CAUSED-BUG-IS-A-SPEC-CASE, P-NAME-THE-REFACTOR-TRAP]
verify_was: n/a
verify_is: test
---

## The bug

`aristo verify --apply-verdicts --rewrite-hashes` rejected one of the 14 dogfood proofs with `cited id 'verify_false_arm_is_intentional_skip' not found in current index`. That id is declared via `aristo::intent_stmt!(...)` inside a `match` arm body in `crates/aristo-cli/src/commands/verify/mod.rs`. The intent existed in source but did not appear in `.aristo/index.toml`, even after a fresh `aristo stamp`.

## Root cause

`walk::extract::Walker::visit_stmt_with_site` was a hand-rolled whitelist of expression kinds whose bodies it descended into when looking for stmt-form macros:

```rust
match stmt {
    syn::Stmt::Macro(m) => self.process_stmt_macro(m, site),
    syn::Stmt::Expr(syn::Expr::Block(b), _) => ...,
    syn::Stmt::Expr(syn::Expr::ForLoop(f), _) => ...,
    syn::Stmt::Expr(syn::Expr::While(w), _) => ...,
    syn::Stmt::Expr(syn::Expr::Loop(l), _) => ...,
    syn::Stmt::Expr(syn::Expr::If(if_expr), _) => ...,
    _ => {}    // ← silently drops everything else
}
```

`syn::Expr` is open-ended: `Match`, `Closure`, `Async`, `Unsafe`, `TryBlock`, `Let`, etc. all contain blocks of statements. None were in the whitelist. So `intent_stmt!` invocations inside `match` arms (and the other unenumerated contexts) were invisible to the indexer.

A fresh `aristo stamp` went from 37 → 39 annotations after the fix: TWO stmt-form intents had been quietly missing the whole time, not just one.

## Fix

Replace the whitelist with `syn::Visit`'s open descent. `Walker` tracks `current_site: Option<String>`; entering an item-level fn or impl-method sets the site and calls `syn::visit::visit_block`, which descends through every `Expr` variant by default. `visit_stmt_macro` uses `current_site` if set.

The Visit-based version is *open by default* — any new `syn::Expr` variant gets visited for free.

## Better (the intent)

> stmt-form intents are discovered via syn::Visit's full descent (visit_block + default traversal of every Expr variant), NOT a hand-rolled whitelist of expression kinds. A whitelist silently drops macros nested inside any unenumerated context — match arms, closures, unsafe blocks, async blocks, try blocks, let initializers — and the failure mode is invisible (the intent doesn't appear in `aristo list`, can't be cited as a ground in a proof, and skips the freshness check). The Visit-based descent is open by default; new syn::Expr variants get visited automatically.

## Verify level

- is: `test`
- reason: regression tests (`extracts_intent_stmt_inside_match_arm`, `..._inside_closure`, `..._inside_unsafe_block`, `..._inside_nested_match_in_let_else`) directly assert the new behavior on each formerly-broken expression kind.

## Why P-ROOT-CAUSED-BUG-IS-A-SPEC-CASE applies here

- Debugging took non-trivial time: a verdict rejection, an `aristo show`, a grep against the index, a grep against the source, then a code-read of the walker to find the whitelist.
- The user pair-debugged with me.
- The fix carries a design lesson: open-ended syn enums + a hand-rolled match arm + a default `_ => {}` is a fragile pattern; this pattern recurs.
- Without the intent, a future "let's simplify this Visit override" refactor could shrink the descent back to a narrow set, and the bug returns silently. The intent + the four regression tests lock the invariant in place.

Discovered while migrating slice 23 dogfood proofs to the validator-fills-hashes schema.
