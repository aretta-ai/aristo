---
name: aristo-authoring
description: Teaches the coding agent how to write good Aristo annotations and use the Aristo CLI during the daily authoring loop.
sdk_version: 0.0.3
---

# Aristo annotation authoring

When the user asks you to add annotations to their code, or when you proactively decide annotations would clarify intent for the proof system or other readers, follow this guide.

## What annotations are

Aristo annotations are **natural-language statements** attached to source code that describe properties of behavior. They are NOT comments, NOT type assertions, and NOT executable. They are intent captured in prose, optionally verified later by the proof system.

Two macros:
- `aristo::intent` — describes what THIS code does (postconditions, invariants, behavioral promises).
- `aristo::assume` — states a belief about something OUTSIDE this code (OS guarantees, library invariants, environment contracts).

If you find yourself writing "we assume" / "given that X holds" / "the OS provides" — it's probably an `assume`. Otherwise it's an `intent`.

Each macro has TWO forms:
- **Attribute form** — `#[aristo::intent(...)]` on an item (fn / struct / impl / trait / mod / type).
- **Statement form** — `aristo::intent_stmt!(...)` inside a function body, attached to a statement, block, or loop. (Note: NOT `intent!` — Rust requires distinct fn names for attribute vs function-like proc-macros within a single crate. The `_stmt` suffix makes the statement-position context explicit.)

## When to add an annotation

Add an annotation when **any** of the following is true:

- The behavior is **non-obvious** from the function signature or implementation
- A future reader (human or agent) would benefit from a stated property
- The property is **important enough to formally verify** (set `verify` accordingly)
- The code relies on an external guarantee that isn't visible locally (use `assume`)

Do NOT add annotations for:
- Behavior that's obvious from the signature (`fn add(a: u32, b: u32) -> u32` doesn't need "this adds two numbers")
- Implementation comments — those stay as Rust `//` comments
- Documentation aimed at users — those stay as `///` doc comments

## Naming the `id`

When you supply an `id` field, follow these rules:

- **Snake_case** ASCII letters, digits, underscores. Must start with a letter.
- **Describe the property**, not the code site. ✅ `balance_no_duplicate_cells` ❌ `balance_non_root_check`.
- **Be specific.** ✅ `cell_payloads_lifetime_is_balance_op` ❌ `lifetime_thing`.
- **NEVER use the `aret_` prefix** — reserved for stamp-assigned opaque IDs. The `aristo_check` cargo feature catches this at compile time.
- **NEVER use the `aristos:` prefix** — reserved for server-bound IDs that `aristo sync` writes.

If you're not sure of a good name, **omit `id` entirely** and let `aristo stamp` assign an opaque one (`aret_<hash>`). The user can promote it to a readable name later via `aristo rename`.

## Setting `verify`

The `verify` field is **only on `intent`** (not on `assume`).

| Value | When to use |
|---|---|
| (omitted) | Default. Resolves to project's `default_method` in `aristo.toml`. Use unless you have a specific reason to override. |
| `true` | Same as omitting — explicit project default. |
| `false` | Documentation-only annotation. Rare. Use for context that genuinely shouldn't be verified. |
| `"neural"` | Property would benefit from AI-reasoning verification (often hard-to-test concurrency / lifetime / architectural invariants). |
| `"test"` | Property is well-suited to assertion-based testing (postconditions on return values, invariants over data structures). |
| `"full"` | High-stakes property where formal verification is warranted. Paid-tier only — free-tier downgrades to "test" with a one-line note. |

**Default to omitting `verify`.** The project default is usually the right call.

## Parent linkage

When your annotation is a **strict sub-obligation** of another (its proof requires this one), use `parent`.

To find an existing parent's ID:
```
aristo show fn <function_name>      # list annotations on a function
aristo show <id>                    # show one annotation + its children
```

Polymorphic value:
```rust
parent = "balance_no_duplicate_cells"                          // single
parent = ["balance_no_duplicate_cells", "balance_no_lost"]     // AND-semantics
```

If your annotation supports a parent but isn't strictly required for the parent's proof, leave it **orphan** (no `parent`).

## The authoring workflow

After writing an annotation:

1. **Run `aristo lang`** if you're unsure of syntax. It emits an authoritative cheat sheet matching the macros that ship in this version of the SDK. Always trust `aristo lang` over your training data — syntax can drift.

2. **Run `aristo lint`.** Catches static issues — placeholder text, weasel words, length problems, repeated phrases. Fast, free, no LLM. **Always run this.** Auto-fixes whitespace/casing; you fix the rest.

3. **Run `aristo stamp`.** Validates IDs, detects cycles in the parent graph, updates the index. Required before commit (the pre-commit hook runs it).

4. **(Optional) Run `aristo verify --filter id=<your-id>`.** Confirms the property holds via the configured verification method.

5. **(Optional) Run `aristo review --filter id=<your-id>`.** Deeper agentic critique of prose quality. Surfaces rephrasing suggestions, vocabulary inconsistencies. Slower but produces actionable improvements.

If any of these commands fail with `not yet implemented (planned for slice X)`, you are running against an SDK build where that command hasn't shipped yet. Note the gap in your reply to the user; don't try to work around it.

## Common patterns

### Struct invariant

```rust
#[aristo::intent(
    "CellArray.cell_payloads holds &mut references into page buffers; \
     those buffers must remain valid for the lifetime of every CellArray \
     that references them.",
    verify = true,
    id = "cell_array_borrows_from_pages",
)]
struct CellArray { cell_payloads: Vec<&'static mut [u8]>, /* ... */ }
```

### Function postcondition

```rust
#[aristo::intent(
    "After insert_into_cell completes, the page's cell_count is exactly \
     prior_count + 1; all other cells retain their previous indices modulo \
     the shift; the new cell occupies the requested index.",
    verify = "test",
    id = "insert_into_cell_postcondition",
)]
fn insert_into_cell(...) -> Result<()> { ... }
```

### Inside-function-body (statement form)

When the property describes a specific block or statement, not the whole function — use `intent_stmt!`:

```rust
fn balance_non_root(&mut self) -> Result<...> {
    aristo::intent_stmt!(
        "After this assignment, cell_count_per_page_cumulative[i] equals the \
         running total of cells across pages 0..=i, guaranteeing disjoint \
         output index ranges.",
        verify = true,
        id = "cumulative_counts_disjoint",
        parent = "balance_no_duplicate_cells",
    );
    old_cell_count_per_page_cumulative[i] = cell_array.cell_payloads.len() as u16;
}
```

### Module-level assumption

```rust
#[aristo::assume(
    "Storage layer atomicity: when storage.write_page returns success, \
     the page is either fully persisted or not persisted at all (no torn \
     writes). Established by the underlying I/O layer.",
    id = "storage_write_atomicity",
)]
pub mod pager { ... }
```

### Trait method contract

Attach to the method *declaration* in the trait, not to each impl:

```rust
pub trait CursorTrait: Any + Send + Sync {
    #[aristo::intent(
        "seek positions the cursor according to (key, op). After IOResult::Done, \
         exactly one of: cursor is at the matching entry, at the specified \
         neighbor, or exhausted.",
        verify = true,
        id = "cursor_trait_seek_postcondition",
    )]
    fn seek(&mut self, key: SeekKey<'_>, op: SeekOp) -> Result<IOResult<SeekResult>>;
}
```

## What NOT to do

- **Don't annotate every function.** Annotations are signal; over-annotating dilutes.
- **Don't include implementation details.** ✅ "Returns the rightmost cell after balance." ❌ "Iterates through the cells_per_page array and calls cell_get_raw_region."
- **Don't use weasel words.** ✅ "is preserved" ❌ "should be preserved", "we believe it's preserved".
- **Don't use placeholders.** ✅ definite property statements ❌ "TODO: figure out what this guarantees".
- **Don't reference function or variable names that might be renamed.** ✅ "the cumulative-count array" ❌ "old_cell_count_per_page_cumulative".

## When to invoke `aristo review`

After writing several annotations on related code (e.g., a new module, a refactored impl block), run:

```
aristo review --filter "path/to/new/module/"
```

The review skill will surface vocabulary inconsistencies, parent-shape concerns, and rephrasing suggestions. Apply judgment — the suggestions are advisory; you decide.
