---
name: aristo-authoring
description: Teaches the coding agent how to write good Aristo annotations and use the Aristo CLI during the daily authoring loop.
sdk_version: {{SDK_VERSION}}
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

**When "why" content is allowed:** only when the design choice itself IS the implicit invariant. "Crashing loudly is better than limping on with a half-built config" is load-bearing — it's the design judgment a refactor would reverse without realizing the implication. "So that lint reformatting doesn't invalidate stamps" is filler motivation; the rule itself is the spec.

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

## Scope: the annotation site is the staleness boundary

The code element an annotation attaches to is not just a label, it is the **scope that triggers re-checking.** Aristo hashes the body of that element; when the body changes, the intent's verification goes stale and is re-checked on the next run. So choosing the site is choosing the boundary at which a code change forces the intent to be re-verified. Place it with deliberation:

- **The site should encompass exactly the code whose change could affect the invariant.** Ask: "if someone edits this code, does the intent need to be re-checked?" The smallest element for which the answer is *yes for every change inside it, and no for changes outside it* is the right site.
- **Too broad is churn.** Annotating a large function, `impl`, or `mod` when the invariant only concerns a few lines means every unrelated edit inside that span marks the proof stale and re-queues a re-check. The user learns to ignore staleness, which defeats the signal.
- **Too narrow is dangerous.** Annotating a tiny helper that does *not* contain the load-bearing logic means real changes to the code that actually governs the invariant never drift the hash, so a now-invalid proof keeps reporting as verified. This is the worse failure: silent staleness.
- **Use the statement form to tighten scope.** When the enclosing function does more than the invariant covers, attach `aristo::intent_stmt!(...)` to the specific statement, block, or loop that carries the property rather than putting the attribute on the whole function. The statement's span becomes the staleness boundary.
- **Use the attribute form when the whole item is the unit.** If any change to the function / struct / impl could plausibly affect the property, the item-level attribute is correct and a tighter scope would miss real drift.

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
    "load_defaults panics rather than returning a Result when the embedded \
     default config fails to parse. That default is compiled into the \
     binary, so a parse failure (extremely rare — usually a broken build) \
     means the binary itself is corrupt. The reasoning: crashing loudly is \
     better than limping on with a half-built config.",
    verify = "test",
    id = "load_defaults_panics_on_corrupt_embedded_config"
)]
```

✅ After (`verify = "neural"` — the load-bearing claim is the design choice):
```rust
#[aristo::intent(
    "The default config is compiled into the binary, so if it fails to \
     parse the binary is corrupt; crashing loudly is better than limping \
     on with a half-built config.",
    verify = "neural",
    id = "load_defaults_panics_on_corrupt_embedded_config"
)]
```

Why: dropped "(extremely rare — usually a broken build)" (commentary) and "The reasoning:" (meta-narrative). Kept the "better than limping on" judgment because that IS the invariant a refactor would reverse ("return Result for good error handling"). Shifted verify to `"neural"` because the claim is a design judgment, not a runtime property.

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

## After a bug is root-caused — encode the lesson as an intent

A bug that took non-trivial debugging to track down is a high-signal trigger to write a new intent. The reason it was missed in the first place is usually that the underlying spec / case was implicit or open-ended; the fix patches *one instance* but the intent locks in the *invariant the bug just demonstrated must hold*.

**When to invoke this trigger:**

- Debugging took meaningful time (not a one-grep find).
- The user pair-debugged with you.
- The fix is more than a typo or import — there's a design lesson in the post-mortem (a default flipped, a fragile mechanism replaced, a case-list expanded).

**When to skip:**

- Surface bugs: typos, off-by-one inside an already-tested function, misnamed variable. The fix + its test are enough; an intent here is noise.
- The new behavior is obvious from the signature (P-CHECK-TYPE-SYSTEM-FIRST applies).

The intent goes on the load-bearing site of the fix (the function, branch, or design choice the bug taught us cannot be casually altered). Its body should:

1. State the invariant directly (P-SPEC-STYLE).
2. Name the failure mode in the language a reviewer would use.
3. Name the refactor that re-introduces the bug, when applicable (P-NAME-THE-REFACTOR-TRAP overlaps strongly here).

Pair the intent with a regression test that asserts the new behavior — that makes `verify = "test"` the natural level. Both the intent (durable design artifact) and the test (mechanical guard) are needed: the test catches the regression, the intent prevents a "let me simplify this" refactor from removing the test along with the guard.

### Worked example

After a dogfood run, one proof rejected with `cited id not found in current index`. Debugging:
1. `aristo show <id>` → not found.
2. grep source → the intent exists, declared via `aristo::intent_stmt!` inside a `match` arm.
3. Read the walker: discovered a hand-rolled whitelist of `syn::Expr` variants (Block, ForLoop, While, Loop, If) that silently dropped `Match`, `Closure`, `Unsafe`, etc.

Fix: replaced the whitelist with `syn::Visit`'s open descent. Then — per this trigger — added:

```rust
#[aristo::intent(
    "stmt-form intents are discovered via syn::Visit's full descent \
     (visit_block + default traversal of every Expr variant), NOT a \
     hand-rolled whitelist of expression kinds. A whitelist silently \
     drops macros nested inside any unenumerated context — match \
     arms, closures, unsafe blocks, async blocks, try blocks, let \
     initializers — and the failure mode is invisible (the intent \
     doesn't appear in `aristo list`, can't be cited as a ground in \
     a proof, and skips the freshness check). The Visit-based \
     descent is open by default; new syn::Expr variants get visited \
     automatically.",
    verify = "test",
    id = "stmt_form_intents_use_open_visit_descent_not_whitelist"
)]
fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) { ... }
```

Plus four regression tests (`extracts_intent_stmt_inside_match_arm`, `..._inside_closure`, `..._inside_unsafe_block`, `..._inside_nested_match_in_let_else`).

Without the intent, a future "let's simplify this Visit override" cleanup could shrink the descent back to a narrow set and the bug returns silently. Without the tests, the intent's claim drifts from the code. Together: durable lesson + mechanical guard.

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

### What to commit — `.aristo/` is tracked, with specific exceptions

`.aristo/` is a tracked project directory, not per-user scratch. **Commit the durable artifacts** in the same commit as the code that changed them:

- `.aristo/index.toml` — the annotation index (regenerated by `aristo stamp`).
- `.aristo/doc/*.md` — per-annotation docs (regenerated by `aristo doc`).
- `.aristo/proofs/*.proof` — durable verification results; they gate trust.
- `aristo.toml` and your annotated source.

The pre-commit hook re-runs `aristo stamp` + `aristo doc`, so stage the resulting `index.toml` / `doc/` changes **with** the source edit that caused them — otherwise the committed tree is internally inconsistent and CI's `stamp --check` / `doc --check` fail.

**Do NOT commit** these — per-user runtime, advisory, or ephemeral, and gitignored in this repo:

- `.aristo/sessions/` — review-session lifecycle, active pointer, rejection log, backlogs (personal audit trail).
- `.aristo/nudge-state.toml` — the reviewed map + nudge throttle.
- `.aristo/verify-queue/`, `.aristo/critique-queue/` — in-flight pipeline tasks.
- `.aristo/critiques/*.critique` — advisory critique findings, regenerated each `aristo critique` run (their dispositions live in the session state).
- `.aristo/proofs/*.proof.bak`, `.aristo/archive/` — transient recovery nets.

Rule of thumb: **results that gate trust or define the index get committed; runtime, review-session, and advisory state do not.**

**Working in another aristo repo?** `aristo init` does not yet wire these ignores for consumer repos, so a freshly-initialized project may surface the runtime/advisory paths above in `git status`. The policy is identical regardless — never stage them. If the repo's `.gitignore` is missing the `.aristo/` block, offer to add it (mirroring the durable-vs-ephemeral split above) rather than committing the noise.

### After a slice closes (deeper review, optional)

8. **`aristo verify --filter id=<your-id>`.** Confirms a runtime claim via the configured verification method. Only meaningful for `verify = "test"` / `"full"` intents.

9. **`aristo review --filter "path/to/new/module/"`.** Deeper agentic critique — vocabulary inconsistencies, parent-shape concerns, rephrasing suggestions. Slower but produces actionable improvements. Apply judgment; suggestions are advisory.

If any of these commands fail with `not yet implemented (planned for slice X)`, you're running against an SDK build where that command hasn't shipped yet. Note the gap in your reply to the user; don't try to work around it.

### The anti-pattern this workflow prevents

You finish a slice with N new functions and zero intents, then promise yourself "I'll do a reflection pass." When you do, the rationale that made each design plausible has been displaced by later decisions; you can no longer reconstruct the WHY, so you either:
- annotate the WHAT (which fails the content gate but ships anyway because nobody re-runs the gate retroactively), or
- decide "no new intents needed for this slice" (which is the default story you tell yourself when the rationale has decayed).

Both outcomes are silent quality losses. The fix is structural: write intents as you write the code, one at a time, in the same edit. The lint+stamp gates catch sloppy prose; only the concurrent-authoring discipline catches missed intents.

## Diff mode — backfill skipped intents on uncommitted changes

Concurrent authoring is the rule. **Diff mode is the one sanctioned exception**: a deliberate retro pass over your **uncommitted changes** to catch intents you skipped while heads-down on the code. Trigger it when the user asks to "backfill / sweep my changes for missed intents", "did I miss any annotations?", or before opening a PR.

**The hard precondition — run it IN-CONTEXT.** Diff mode is only legitimate in the *same session that wrote the changes*, while the rationale is still in your head. The whole reason batching is banned is that the *why* decays; a retro pass recovers it only if you still remember it. So:

- If you wrote this code earlier this session → you can recover the *why* → diff mode is valid.
- If you're a fresh agent with no memory of these changes (e.g., picking up someone else's working tree) → you can only see the *what* → **do NOT run diff mode.** You'd manufacture WHAT-annotations that fail the content gate. Say so and stop.

### Mechanism

1. **Find the changed code.** Scope is the working tree + staged changes (NOT committed history):
   ```bash
   git diff --stat HEAD            # overview of what changed
   git diff HEAD                   # the actual hunks (working tree + staged), new + modified code
   ```
   Read the hunks, not just the file names — you're looking at the specific functions / blocks / decisions that changed.

2. **Find what's already covered vs. what drifted.** Cross-reference the index so you don't re-annotate covered sites and so you catch claims the change may have invalidated:
   ```bash
   aristo stamp --check            # body-drifted entries = annotations whose covered code changed
   aristo show fn <name>           # what's already annotated on a changed function
   ```
   A **body-drifted** existing intent is a flag, not a backfill target: re-read it against the new code and decide whether the change *upheld* the claim (fine — re-stamp) or *invalidated* it (the claim must be rewritten or the code fixed — that's a verify concern, route it to `/aristo-verify`, don't silently re-stamp a now-false claim).

3. **Per changed site lacking an annotation, apply the content gate HARD.** This is a retro pass, so be *stricter* than usual — the default answer is "no intent needed." Most changed lines are mechanical and earn nothing. Only a design decision that (a) a sharp reader would miss from the code alone AND (b) a plausible refactor could break silently passes. If you can't articulate the *why* from memory, that's the signal it either wasn't load-bearing or the rationale already decayed — skip it.

4. **Propose, confirm, then write — no silent edits.** For each candidate that passes the gate, surface it via `AskUserQuestion` (the proposed intent text + its site in the `preview`) and write it only after the user confirms. Attach it to the load-bearing site, inline, exactly as in concurrent authoring. Batch the *proposals* into a navigable set (≤4 per page), but each is an individual decision — never bulk-insert.

5. **Validate.** After the accepted intents are written: `aristo lint --check` → `aristo stamp`. Same gates as the normal loop.

### Why diff mode is allowed when batching isn't

Batching-at-end-of-slice fails because the rationale is *gone* by then. Diff mode is bounded differently: it runs **in the authoring session**, it applies the gate **harder** (default = skip), and it **confirms every write** — so it recovers genuinely-skipped intents without becoming a WHAT-annotation generator. If those three guardrails don't hold (fresh context, soft gate, or silent bulk insert), you're just batching, and the quality loss this whole skill warns about comes right back.
