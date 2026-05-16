---
name: aristo-authoring
description: Teaches the coding agent how to write good Aristo annotations and use the Aristo CLI during the daily authoring loop.
sdk_version: 0.0.4
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
- **Statement form** — `aristo::intent_stmt!(...)` inside a function body, attached to a statement, block, or loop. (Note: NOT `intent` as a bang macro — Rust requires distinct fn names for attribute vs function-like proc-macros within a single crate. The `_stmt` suffix makes the statement-position context explicit.)

## When: as you write the code, not after

**Write intents inline with the code that motivates them — not in a sweep at the end of a slice.** Each time you make a design choice that another reader could miss or reverse, apply the content gate immediately. Either pass the gate (write the intent there and then) or skip and move on. **Never batch.**

Why this matters:

- **Rationale decays fast.** Five functions later, you'll remember WHAT the code does (the type signature already tells you) but not WHY you took the silent-skip path over erroring, or why you fixed the rule order. End-of-slice annotation passes systematically produce WHAT-annotations that fail the content gate.
- **Intents are your chain-of-thought made visible.** When you choose between two implementations and pick one for a non-obvious reason, that reason IS the intent. Writing it down as you decide:
  - Forces you to articulate the load-bearing claim, which sharpens the decision.
  - Gives the human reviewing your diff a preview of the design judgment, *before* the code lands, in the same hunk as the code.
  - Closes the loop with `aristo lint`, which will reject placeholder or weasel-worded intents written under deadline pressure.
- **End-of-slice retros catch what slipped, not what was inevitable.** Reflection is for noticing the pattern across slices, not for the first-pass authoring of any individual intent.

**The trap to avoid:** "I'll add the `#[aristo::intent]` annotations after I get the code working." By then the code DOES work, and the rationale that made one design plausible over another has been displaced by the next decision. You end up annotating effects ("returns true when X") instead of intents ("X is rejected here because Y silently breaks Z").

**Working pattern:** when you'd otherwise type a `//` comment explaining a non-obvious choice, ask: is this an intent? If yes (gate passes), write it as an intent attached to the load-bearing site. If no (just narrating WHAT), don't write the comment either — let the names do the work.

## Before writing: the content gate

Apply this AT each design-decision point as you write, not as a retro pass. The content gate filters out annotations that look reasonable but add no value.

**An intent makes explicit something that lives implicitly in the programmer's mind and is invisible from the code alone.** Typically:

- An invariant a refactor could subtly break without compile-error or test-failure feedback.
- A design choice that *looks* incomplete or wrong from outside, so a reader (human or agent) might "fix" it and regress the system.
- Cross-cutting context an agent would otherwise reverse-engineer from tests, comments, or git archaeology.

For every candidate annotation, ask:

1. **Would a sharp reader of the code alone miss this?** If the property is obvious from the type signature, function name, or function body — don't write the intent.
2. **Could a plausible refactor break this silently?** "Silently" is key: if Rust's type system, exhaustive matching, or test failures would catch the regression, the type system is already doing the work.
3. **Does this save reverse-engineering effort?** If a new contributor or agent would otherwise have to read tests, callers, or commit history to recover this knowledge — that's where intents earn their keep.

If both answers to (1) and (2) are no, **don't write the intent.** A perfectly-worded intent that fails the gate still adds noise.

## The shape of a good intent

Write intents as English sentences with the precision of a spec. Closer to POSIX man pages, W3C normative language, Postgres documentation, or TigerBeetle's design docs than to formal logic.

**Use:**
- Direct invariant statements ("every byte inside the covered region is significant").
- Concrete domain nouns ("the covered region", "canonical form", "opaque id").
- Natural-language quantification ("every byte", "any change", "no leading whitespace").
- Normative keywords (MUST / MAY) sparingly, only for actual caller contracts.

**Avoid:**
- Motivation prose: "so that lint-induced reformatting…", "the way this works…", "this lets us…".
- Narration: "first it walks the tree, then it…", "tests rely on this to assert…".
- Examples inside the intent body: "(re-wrapping a long string, fixing indentation)".
- Weasels: "usually", "typically", "by design".
- Formulas, regex, ∀-quantifiers, function-call syntax in the body.
- Code identifiers (function names, type names) where domain nouns work. Identifiers rename; concepts don't.

**Why prose-spec, not formal logic:** the audience is everyday developers (and other agents), not formal-methods experts. A Coq-style formula alienates the reader. A direct English sentence stating the invariant is precise enough.

**When "why" content is allowed:** only when the design choice itself IS the implicit invariant. "A low-entropy id silently committed would be worse than a failed run the user can retry" is load-bearing — it's the design judgment a refactor would reverse without realizing the implication. "So that lint reformatting doesn't invalidate stamps" is filler motivation; the rule itself is the spec.

## Setting `verify`

The `verify` field is **only on `intent`** (not on `assume`).

Pick the level based on the **verifiability shape of the load-bearing claim** — not the importance of the intent, and not the testability of side claims.

| If the load-bearing claim is… | Use |
|---|---|
| A runtime property a mined assertion or test could catch (postconditions, equivalence classes, round-trips, ordering invariants) | `verify = "test"` |
| A design decision, a refactor-trap warning, or "intentional, not incomplete" guidance — reviewable by reading the code, not reducible to a runtime check | `verify = "neural"` |
| A formal-proof candidate (algorithmic invariant amenable to a solver) | `verify = "full"` (paid tier) |
| Pure coordination convention with no checkable shape | `verify = false` |
| You're not sure and the project default is the right call | omit `verify` (or `verify = true`, same effect) |

**Over-marking design-philosophy intents as `"test"` is the most common mistake.** It pollutes the verification pipeline with permanently-unverifiable entries; the user sees `status=unknown` forever and learns to ignore the signal.

**Under-marking testable invariants as `"neural"` wastes the testing pipeline's stronger signal.** When a property reduces to a clean runtime assertion, prefer `"test"`.

**Coupled rule:** if the intent body relies on "why" content to be load-bearing (a design judgment), it's almost certainly a `"neural"` intent, not a `"test"` one.

## Where the annotation goes

An invariant lives on the function that ENFORCES it, not on every caller that BENEFITS from it. Duplicating the same property across sites adds noise and confuses the reader about which annotation is authoritative.

If you find yourself writing the same invariant twice on different functions in the same call chain, the right call is usually:
- Keep it on the lower-level enforcement site (the one whose code would have to change to break the invariant).
- Delete it from the higher-level orchestration site.

## One annotation, one invariant

If a draft body has two distinct invariants, split them into two annotations OR move one to a more appropriate site. Mixed intents read as motivation prose and lose precision in both halves.

**Exception:** two claims that share one function AND are both about the same domain layer (e.g., both about file-system semantics of one write operation) can stay together if combining keeps the body tight.

## Naming the `id`

When you supply an `id` field, follow these rules:

- **Snake_case** ASCII letters, digits, underscores. Must start with a letter.
- **Describe the property**, not the code site. ✅ `balance_no_duplicate_cells` ❌ `balance_non_root_check`.
- **Be specific.** ✅ `cell_payloads_lifetime_is_balance_op` ❌ `lifetime_thing`.
- **NEVER use the `aret_` prefix** — reserved for stamp-assigned opaque IDs. The `aristo_check` cargo feature catches this at compile time.
- **NEVER use the `aristos:` prefix** — reserved for server-bound IDs that `aristo sync` writes.

If you're not sure of a good name, **omit `id` entirely** and let `aristo stamp` assign an opaque one (`aret_<hash>`). The user can promote it to a readable name later via `aristo rename`.

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

## Common patterns

### Struct invariant

```rust
#[aristo::intent(
    "Every CellArray.cell_payloads reference points into a page buffer \
     that outlives the CellArray. Dropping a page while its CellArray is \
     still live would dangle the references.",
    verify = "neural",
    id = "cell_array_borrows_from_pages",
)]
struct CellArray { cell_payloads: Vec<&'static mut [u8]>, /* ... */ }
```

### Function postcondition

```rust
#[aristo::intent(
    "After insert_into_cell returns successfully, the page's cell_count \
     is exactly one greater than before; the new cell occupies the \
     requested index; all other cells shift up by one without reordering.",
    verify = "test",
    id = "insert_into_cell_postcondition",
)]
fn insert_into_cell(...) -> Result<()> { ... }
```

### Inside-function-body (statement form)

```rust
fn balance_non_root(&mut self) -> Result<...> {
    aristo::intent_stmt!(
        "After this assignment, the cumulative-count entry at index i \
         equals the running total of cells across pages 0..=i. Disjoint \
         output index ranges depend on this.",
        verify = "test",
        id = "cumulative_counts_disjoint",
        parent = "balance_no_duplicate_cells",
    );
    /* code that updates the cumulative count */
}
```

### Module-level assumption

```rust
#[aristo::assume(
    "Storage layer atomicity: when storage.write_page returns success, \
     the page is either fully persisted or not persisted at all — no \
     torn writes. Established by the underlying I/O layer.",
    id = "storage_write_atomicity",
)]
pub mod pager { ... }
```

### Trait method contract (attach to the declaration, not impls)

```rust
pub trait CursorTrait: Any + Send + Sync {
    #[aristo::intent(
        "After seek returns IOResult::Done, the cursor is at exactly one \
         of: the matching entry, the specified neighbor, or exhausted. \
         These three states are mutually exclusive.",
        verify = "test",
        id = "cursor_trait_seek_postcondition",
    )]
    fn seek(&mut self, key: SeekKey<'_>, op: SeekOp) -> Result<IOResult<SeekResult>>;
}
```

## Real-world rewrites — before / after

Concrete pairs from the SDK's own dogfood audit. Same content in each pair; the "after" form is tighter and more refactor-trap-naming.

### Cut motivation prose; lead with the invariant

❌ Before:
```rust
#[aristo::intent(
    "text_hash normalizes whitespace before hashing so that lint-induced \
     reformatting (re-wrapping a long string, fixing indentation) doesn't \
     invalidate stamped annotations. The mapping is: trim ends, then \
     collapse runs of ASCII whitespace into a single space.",
    verify = "test",
    id = "text_hash_normalizes_whitespace"
)]
```

✅ After:
```rust
#[aristo::intent(
    "Whitespace differences in annotation text — leading, trailing, or \
     runs collapsed to one space — do not change the text hash. \
     Reformatting prose is not drift.",
    verify = "test",
    id = "text_hash_normalizes_whitespace"
)]
```

Why: dropped "so that lint-induced reformatting…" (motivation), dropped the parenthetical example, dropped "The mapping is:" (filler). Same content, easier to scan.

### Keep "why" when the design choice IS the invariant; shift verify level accordingly

❌ Before (`verify = "test"`, but no test can capture "panic is the right failure mode"):
```rust
#[aristo::intent(
    "generate_opaque_id always returns a parseable AnnotationId with the \
     `aret_` prefix. The OS RNG (getrandom) is the source of entropy; if \
     it fails (extremely rare — usually a misconfigured kernel), this \
     function panics rather than returning a Result. The reasoning: a \
     stamped id with weak entropy is worse than a crashed run.",
    verify = "test",
    id = "generate_opaque_id_always_parses"
)]
```

✅ After (`verify = "neural"` — the load-bearing claim is the design choice):
```rust
#[aristo::intent(
    "Opaque ids carry enough entropy that collisions across a project are \
     negligible. If the OS can't produce randomness, the stamp crashes; a \
     low-entropy id silently committed would be worse than a failed run \
     the user can retry.",
    verify = "neural",
    id = "generate_opaque_id_always_parses"
)]
```

Why: dropped "(extremely rare — usually a misconfigured kernel)" (commentary) and "The reasoning:" (meta-narrative). Kept the "would be worse than a failed run" judgment because that IS the invariant a refactor would reverse ("return Result for good error handling"). Shifted verify to `"neural"` because the claim is a design judgment, not a runtime property.

### Name the refactor trap explicitly

❌ Before (narrates callers; doesn't tell the reader what NOT to do):
```rust
#[aristo::intent(
    "extract_from_source returns annotations in source order (top of file \
     first). Tests rely on this ordering to assert specific entries by \
     index without selector machinery, and the downstream walker depends \
     on it for stable index.toml ordering when ids haven't been assigned.",
    verify = "test",
    id = "extract_returns_annotations_in_source_order"
)]
```

✅ After (states the invariant, then names the refactor that would break it):
```rust
#[aristo::intent(
    "Annotations return in source order — top of file first. Sorting or \
     hashing the result would silently break stable index ordering and \
     the test fixtures that index into it positionally.",
    verify = "test",
    id = "extract_returns_annotations_in_source_order"
)]
```

Why: the "sorting or hashing the result would silently break" phrase speaks the language of the change someone is about to make ("let's use a HashMap for O(1) lookups"). Naming the trap stops the refactor before it lands.

### Use "intentional, not incomplete" when the design stops short

❌ Before (looks like an unfinished function from outside; agent would propose "let me complete it"):
```rust
#[aristo::intent(
    "detect_cycles returns the FIRST cycle it finds and stops; it does \
     not enumerate all cycles in the graph. The diagnostic-friendly path \
     is enough for the user to break the cycle and re-run; chasing every \
     cycle on the same pass would multiply diagnostic noise without \
     helping the fix.",
    verify = "test",
    id = "detect_cycles_returns_first_cycle_only"
)]
```

✅ After (explicit "intentional, not incomplete" disarms the "let me fix this" instinct):
```rust
#[aristo::intent(
    "One cycle reported per call, then return. This is intentional, not \
     incomplete — extending to enumerate all cycles would multiply \
     diagnostic noise without helping the fix.",
    verify = "neural",
    id = "detect_cycles_returns_first_cycle_only"
)]
```

Why: the three words "intentional, not incomplete" prevent an entire class of well-intentioned regressions where a reader sees the function "looking incomplete" and tries to extend it. Shifted verify to `"neural"` because the claim is the design intent, not a runtime invariant.

## Anti-patterns — what NOT to do

- ❌ **Don't restate what the type system already enforces.** Rust's exhaustive `match` on a closed enum cannot silently omit an arm — the compiler errors. Don't write intents claiming "a missing arm would silently fail" — that's factually wrong about Rust and adds zero value.
- ❌ **Don't duplicate the same invariant on caller and callee.** Pick the lower-level enforcement site; delete from the orchestration site.
- ❌ **Don't annotate trivia.** `fn add(a: u32, b: u32) -> u32` doesn't need "this adds two numbers." The signature already says it.
- ❌ **Don't include implementation details.** ✅ "Returns the rightmost cell after balance." ❌ "Iterates through the cells_per_page array and calls cell_get_raw_region."
- ❌ **Don't use weasel words.** ✅ "is preserved" ❌ "should be preserved", "we believe it's preserved", "by design".
- ❌ **Don't use placeholders.** ✅ definite property statements ❌ "TODO: figure out what this guarantees".
- ❌ **Don't reference function or variable names that might be renamed.** ✅ "the cumulative-count array" ❌ "old_cell_count_per_page_cumulative".
- ❌ **Don't mark design judgments as `verify = "test"`.** No test will ever be derived; you'll just pollute the verification pipeline with `status=unknown` forever.
- ❌ **Don't pile two invariants into one intent.** Split them, or move one to its proper site.

## The authoring workflow

### While writing the code (concurrent — not retro)

For every design decision point you reach:

1. **Apply the content gate** (see top of this file). If the rationale would be invisible to a sharp reader and a plausible refactor could break it silently → pass the gate.
2. **Write the intent inline.** Attach it to the load-bearing site (the function / struct / impl that enforces the property), in the same edit as the code itself. Not at the bottom of the file, not in a follow-up commit.
3. **Speak the intent in your reply to the user**, briefly. One line is fine: "Adding intent on `apply_autofix`: the count is rule-applications, not anomaly count, so swapping the rule order changes user-visible output." This is your chain-of-thought, made visible — and it gives the human a preview of the design judgment in the same turn as the code lands.
4. **Move on.** Don't pre-write tests or doc comments for the intent. One step, smallest sensible scope.

If the candidate fails the gate, say so briefly and skip — don't write a comment in its place either.

### Before each commit (validation, fast)

5. **`aristo lang`** if you're unsure of syntax. Authoritative cheat sheet for the macros this SDK version ships. Trust it over your training data.

6. **`aristo lint --check`.** Catches placeholder text, weasel words, length problems. Fast, free, no LLM. **Always run this** — the pre-commit hook runs it anyway, so failures abort the commit. Use `aristo lint --fix` to auto-resolve whitespace issues (doubled spaces, trailing whitespace); fix the rest by hand.

7. **`aristo stamp`.** Validates IDs, detects parent-graph cycles, updates the index. Also pre-commit-gated.

### After a slice closes (deeper review, optional)

8. **`aristo verify --filter id=<your-id>`.** Confirms a runtime claim via the configured verification method. Only meaningful for `verify = "test"` / `"full"` intents.

9. **`aristo review --filter "path/to/new/module/"`.** Deeper agentic critique — vocabulary inconsistencies, parent-shape concerns, rephrasing suggestions. Slower but produces actionable improvements. Apply judgment; suggestions are advisory.

If any of these commands fail with `not yet implemented (planned for slice X)`, you're running against an SDK build where that command hasn't shipped yet. Note the gap in your reply to the user; don't try to work around it.

### The anti-pattern this workflow prevents

You finish a slice with N new functions and zero intents, then promise yourself "I'll do a reflection pass." When you do, the rationale that made each design plausible has been displaced by later decisions; you can no longer reconstruct the WHY, so you either:
- annotate the WHAT (which fails the content gate but ships anyway because nobody re-runs the gate retroactively), or
- decide "no new intents needed for this slice" (which is the default story you tell yourself when the rationale has decayed).

Both outcomes are silent quality losses. The fix is structural: write intents as you write the code, one at a time, in the same edit. The lint+stamp gates catch sloppy prose; only the concurrent-authoring discipline catches missed intents.
