# Aristo API spec — for agents in a Rust codebase

**Audience:** an AI coding agent operating in a fresh Rust project that wants to annotate code with **aristo intents** (logical-layer claims) and instrument it with **aristo instrumentation** macros (mechanical-layer observability + fault injection).

**Pinned to:** `aristo` v0.2.6. Surface is stable within v0.2.x.

This is a complete reference. With this doc + standard Rust knowledge, you can write idiomatic aristo code from scratch. For the working-pattern guidance (when to annotate vs not, content gates, common pitfalls), see the companion skill `aristo-instrumenting` and the conventions docs cross-referenced at the end.

---

## 0. Setup

### `Cargo.toml`

```toml
[dependencies]
aristo = "0.2"
# Add only the features you actually use. None of these are default-on
# at the consumer level — the consumer's Cargo.toml controls activation.
# Defaults are conservative.

[features]
# Recommended pattern: alias the instrument flag onto a project-local
# name. Consumers of YOUR crate enable instrumentation by toggling YOUR
# name, never the aristo flag directly.
differential-accessors = ["aristo/aristo_instrument"]
```

### Feature flags

| Flag | What it gates | Default | Cost when off |
|---|---|---|---|
| `aristo_check` | Validates `intent` / `assume` argument shapes at compile time | **on** | Validation skipped; macros still expand correctly |
| `aristo_doc` | Injects `.aristo/doc/<id>.md` content above each annotated item via rustdoc `#[doc = include_str!(...)]` | off | No `cargo doc` content injection |
| `aristo_instrument` | Exposes the three instrument macros (`Inspect`, `expose_pub`, `yield_point!`) + runtime hook (`set_hook`, `__yield_point`) | off | The whole `aristo::instrument` module doesn't exist; using the macros fails at name resolution |

Turn `aristo_instrument` on transitively via your project-local alias (`differential-accessors`, `harness`, whatever fits) — never depend on it directly at consumer call sites.

### Optional: install the authoring skills

```sh
cargo install aristo-cli
aristo install-skills --agent claude-code           # or cursor / codex / opencode / antigravity
```

This installs the bundled skills (`aristo-authoring`, `aristo-instrumenting`, `aristo-verify`, ...) into your agent's skill directory. Skills give the agent context on when and how to use each macro — useful but not required for the macros to work.

---

## 1. The two layers

Aristo separates verification into two distinct surfaces:

| Layer | Surface | What you claim | When you reach for it |
|---|---|---|---|
| **Logical** | `aristo::intent`, `aristo::assume` | *what the code does* (postconditions, invariants, environmental contracts) | Whenever you've made a non-obvious design decision that a future reader / refactor could silently reverse |
| **Mechanical** | `aristo::instrument::Inspect`, `::expose_pub`, `::yield_point!` | *what the code's state is observable as* | Whenever a verification or differential-testing harness needs to read or affect private state |

The two layers are independent: turn on `aristo_check` for logical claims, `aristo_instrument` for mechanical observability — neither implies the other.

---

## 2. `aristo::intent` — claims about *this* code

Use when you want to record a property of the function/module/struct that future readers (human or agent) could miss from the code alone.

### 2.1 Attribute form

Applies to items: `fn`, `struct`, `enum`, `impl`, `trait`, `mod`, `type`.

```rust
#[aristo::intent("returns Some(value) iff key is present; never panics")]
pub fn get(&self, key: &str) -> Option<&str> { /* ... */ }
```

### 2.2 Statement form

For when the claim attaches to a statement, block, or loop inside a function body, not the whole function.

```rust
fn process(items: &[Item]) -> Vec<Output> {
    let mut out = Vec::with_capacity(items.len());
    for it in items {
        aristo::intent_stmt!("items in `out` preserve `items`' order");
        out.push(transform(it));
    }
    out
}
```

The `_stmt` suffix is required (not `intent!` — Rust E0428 forbids re-using a name across attribute and function-like proc-macros in the same crate).

### 2.3 Argument schema

```
#[aristo::intent("<text>", verify = <mode>, parent = <id>, id = "<id>")]
```

| Arg | Required | Type | Values / shape |
|---|---|---|---|
| `<text>` (first positional) | yes | string literal | non-empty after `.trim()`; one sentence describing the claim |
| `verify` | optional | bool literal OR string literal | `false` \| `"false"` \| `"test"` \| `"neural"` \| `"full"` \| `true` |
| `parent` | optional | expression | identifier or string literal naming the parent intent's id |
| `id` | optional | string literal | snake_case (lowercase ASCII, digits, underscores; first char a letter); MUST NOT start with `aret_` or `aristos:` (tool-managed namespaces) |

Examples covering each arg:

```rust
// Minimum: just the claim.
#[aristo::intent("idempotent — calling twice has the same effect as once")]
fn install(&mut self) -> Result<()> { /* ... */ }

// With verify mode (neural verification by the aristo CLI).
#[aristo::intent(
    "returns Err(IoError) only when the underlying file write fails",
    verify = "neural",
)]
fn flush(&mut self) -> Result<()> { /* ... */ }

// With an explicit id (for parent references) and parent.
#[aristo::intent(
    "removes the entry; returns the prior value if present",
    id = "map_remove_returns_prior",
    parent = "map_invariants",
)]
fn remove(&mut self, k: &K) -> Option<V> { /* ... */ }
```

### 2.4 When to use

Apply at the *load-bearing decision site*. Good targets:

- A function whose correctness depends on a non-obvious invariant.
- A struct or trait that upholds a system-wide property.
- An impl that handles a specific case differently for a deliberate reason.
- Anywhere a plausible refactor could silently break behavior.

**Don't apply to:**

- Trivial getters / setters / `From` impls / `Display` impls.
- Pure data containers (`struct Point { x: f64, y: f64 }`).
- Anywhere the function name + signature already says everything.

### 2.5 Verification modes

| Mode | Meaning | Cost |
|---|---|---|
| `false` (or omitted) | Not verifiable — just documentation | None |
| `"test"` | Verified by an annotated test | You write the test |
| `"neural"` | Verified by the aristo neural verifier (LLM-driven) | `aristo verify` invocation; signs in to the server |
| `"full"` | Verified by a formal proof (Lean-driven) | Same, deeper |

Most user-authored intents are `verify = "neural"` (or unspecified).

---

## 3. `aristo::assume` — claims about *outside* this code

Same shape as `intent` minus `verify` (assumptions describe invariants you rely on, not properties to be verified).

```rust
#[aristo::assume("clock is monotonic — successive calls never return earlier than prior")]
pub fn now(&self) -> Instant { /* ... */ }
```

Statement form:

```rust
fn parse(input: &[u8]) -> Result<Frame> {
    aristo::assume_stmt!("input has already passed the magic-bytes check");
    // ... rely on that
}
```

### Argument schema

```
#[aristo::assume("<text>", parent = <id>, id = "<id>")]
```

Same as `intent` minus the `verify` arg. **Passing `verify` on `assume` is a hard error** — the parser rejects it with a hint to use `intent` instead. This is a category check, not a styling preference: assumptions aren't verification targets.

### When to use

- An OS-level guarantee (filesystem ordering, signal delivery, etc.).
- A library contract you don't control.
- A caller invariant guaranteed elsewhere (`mod`-level pre-condition).
- Anywhere you'd write "we assume X" in a comment.

---

## 4. `aristo::instrument::Inspect` — snapshot derive

Generates a snapshot accessor on a struct so a test harness can read the contents of selected fields as an owned, point-in-time `Vec` (or owned value).

### 4.1 v0.2.6 reality check

The locked design is type-agnostic, but the v0.2.6 implementation supports **`SkipMap<K, V>` fields only**. Tagging any other field type (`BTreeMap`, `HashMap`, `Vec`, scalars, atomics) produces the macro-level error `"only supports SkipMap<K, V> fields in v1"`. This is **implementation debt to close**, not deferred design — see `docs/decisions/instrument-surface.md` § "Implementation debt". For non-SkipMap fields today, hand-write the accessor.

### 4.2 Surface

```rust
#[derive(aristo::instrument::Inspect)]
pub struct Store {
    #[inspect]                     // CLONE mode
    files: SkipMap<u64, File>,

    #[inspect(FileView)]           // PROJECT mode (positional T)
    archived: SkipMap<u64, File>,

    #[inspect(name = "states")]    // rename method
    state_records: SkipMap<u64, State>,
}
```

### 4.3 Modes

| Form | Mode | Generated method | Required impls on V |
|---|---|---|---|
| `#[inspect]` | clone | `pub fn inspect_<field>(&self) -> Vec<(K, V)>` | `V: Clone` |
| `#[inspect(T)]` | project | `pub fn inspect_<field>(&self) -> Vec<(K, T)>` | `impl From<&V> for T` |
| `#[inspect(name = "x")]` | clone + rename | `pub fn inspect_x(&self) -> Vec<(K, V)>` | `V: Clone` |
| `#[inspect(T, name = "x")]` | project + rename | `pub fn inspect_x(&self) -> Vec<(K, T)>` | `impl From<&V> for T` |

K must satisfy `K: Copy` in all forms (the generated body dereferences `*entry.key()` from the SkipMap entry).

### 4.4 Examples

**Clone mode** — when V is `Clone`:

```rust
use aristo::instrument::Inspect;
use crossbeam_skiplist::SkipMap;

#[derive(Clone)]
pub struct Transaction { pub seq: u64, pub status: TxStatus }

#[derive(Inspect)]
pub struct MvStore {
    #[cfg_attr(feature = "differential-accessors", inspect)]
    txs: SkipMap<u64, Transaction>,
}

// Harness side:
let snap: Vec<(u64, Transaction)> = store.inspect_txs();
```

**Project mode** — when V holds non-Clone internals OR needs canonicalization:

```rust
pub struct Transaction {
    pub seq: u64,
    pub locks: Arc<Mutex<Vec<LockHandle>>>,   // not Clone
}

pub struct TxnSnapshot { pub seq: u64 }       // harness-visible fields only

impl From<&Transaction> for TxnSnapshot {
    fn from(t: &Transaction) -> Self { TxnSnapshot { seq: t.seq } }
}

#[derive(Inspect)]
pub struct MvStore {
    #[cfg_attr(feature = "differential-accessors", inspect(TxnSnapshot))]
    txs: SkipMap<u64, Transaction>,
}
```

### 4.5 Semantics

- Untagged fields are silently ignored (no accessor, no error).
- The returned `Vec` is **owned and point-in-time** — the harness can't write back to the SUT through it.
- No automatic sort — consumers sort in test code if Lean-comparison canonicalization requires it.
- The macro emits `impl <generics> StructName <ty_generics> <where_clause>` correctly; generic structs work as of v0.2.6.

### 4.6 Errors the macro can produce

| Trigger | Diagnostic |
|---|---|
| Non-`SkipMap` field type | `` `#[inspect(...)]` only supports `SkipMap<K, V>` fields in v1 `` |
| Tuple struct (no named fields) | `` `#[derive(Inspect)]` requires a struct with named fields `` |
| Multiple `#[inspect(...)]` on one field | `multiple `#[inspect(...)]` attributes on one field; combine them` |
| `#[inspect(...)]` with unknown kwarg | `unknown `inspect` argument; expected positional `T` (projection type) or `name = "..."` ` |

---

## 5. `aristo::instrument::expose_pub` — visibility raise

Raises the visibility of a `pub(crate)` item to `pub` so a cross-crate harness can construct / call / reference it. Three forms based on the annotated item kind.

### 5.1 Function form

```rust
mod inner {
    use aristo::instrument::expose_pub;

    impl Buf {
        #[cfg_attr(feature = "differential-accessors",
                   expose_pub(as = "new_for_test"))]
        pub(crate) fn new(capacity: usize) -> Self { /* ... */ }
    }
}

// Harness side (with feature on):
let b = inner::Buf::new_for_test(64);
```

| Aspect | Rule |
|---|---|
| `as = "<name>"` | **required** — a distinct wrapper name |
| Original | preserved unchanged (still `pub(crate)`) |
| Wrapper | `pub` + `#[doc(hidden)]`; same signature as original |
| Call shape | `self.X(...)` (with receiver), `Self::X(...)` (associated fn in impl), `X(...)` (free function) |
| Convention | name the wrapper with a `_for_test` suffix |

Works for free `fn`, methods with any receiver (`&self` / `&mut self` / `self`), and associated functions. Generic parameters, lifetimes, and `where` clauses pass through verbatim.

### 5.2 Type form

```rust
#[cfg_attr(feature = "differential-accessors", aristo::instrument::expose_pub)]
pub(crate) enum ParsedOp {
    Get(u64),
    Put(u64, Vec<u8>),
}
```

| Aspect | Rule |
|---|---|
| `as = "..."` | **forbidden** — rename of a type breaks every reference |
| Original | replaced in place with `pub` visibility + `#[doc(hidden)]` |
| Behavior | with feature off, original `pub(crate)` stands; with feature on, the type becomes `pub` |

Works on `enum`, `struct`, `type` alias.

### 5.3 Impl-block form

```rust
#[cfg_attr(feature = "differential-accessors", aristo::instrument::expose_pub)]
impl Counter {
    pub(crate) fn bump(&mut self) { self.n += 1 }
    pub(crate) fn read(&self) -> u64 { self.n }
    pub(crate) const ZERO: u64 = 0;   // ← untouched (non-`fn` items left alone)
}
```

Every `fn` inside gets visibility raised to `pub` + `#[doc(hidden)]`. Associated consts / types are not affected. `as = "..."` is forbidden (same reason as the type form).

### 5.4 Errors the macro can produce

| Trigger | Diagnostic |
|---|---|
| `#[expose_pub]` on a fn without `as = "..."` | `` `#[expose_pub]` on a function requires `as = "<wrapper_name>"` `` |
| `#[expose_pub(as = "...")]` on a type/impl | `` `#[expose_pub]` on a type / impl-block does not accept arguments `` |
| Unknown attribute arg | `unknown `expose_pub` argument; expected `as = "<wrapper_name>"`` |
| Destructuring pattern in fn arg | `` `#[expose_pub]` v1 supports plain identifier args; destructuring patterns are not yet supported `` |

---

## 6. `aristo::instrument::yield_point!` + runtime hook

Inserts a labeled fault-injection point that a test harness can hook into to pause / inject failures / re-order.

### 6.1 Surface (at the call site)

```rust
fn write_header(&mut self) -> std::io::Result<()> {
    self.header.version = self.new_version;
    #[cfg(feature = "differential-accessors")]
    aristo::instrument::yield_point!("write_header.before_fsync");
    self.pwrite(&header_bytes, 0)?;
    self.file.sync_all()?;
    Ok(())
}
```

The macro expands to a call into `aristo::instrument::__yield_point("write_header.before_fsync")`. The expansion is unconditional — gate the call site yourself with `cfg!` or `#[cfg(feature = "...")]` so production builds don't carry the call.

Label rules:

- Must be a string literal (the runtime hook takes `&'static str` to keep the hot path allocation-free).
- Convention: `<fn-name>.before_<action>` or `<fn-name>.after_<action>` (e.g., `"commit.after_log_sync"`).
- One label per call site within a function — the harness selects which point to inject by string match.

### 6.2 Runtime hook (the harness side)

```rust
pub fn aristo::instrument::set_hook(hook: Option<fn(&'static str)>);
pub fn aristo::instrument::__yield_point(label: &'static str);    // internal; macro expands to this
```

| Signature | Meaning |
|---|---|
| `set_hook(Some(callback))` | install a thread-local callback invoked at every yield point |
| `set_hook(None)` | clear the installed callback (silent no-op for subsequent yield points) |
| `__yield_point(label)` | dispatched by the macro; user code never calls this directly |

The hook is `fn(&'static str)`, not `Fn` or `FnMut` — keeps the hot path allocation-free at the cost of no closures. Use static state (`Atomic*`, `thread_local!`) for the harness to read out which labels fired.

### 6.3 Harness pattern

```rust
use aristo::instrument::set_hook;
use std::cell::RefCell;

thread_local! {
    static OBSERVED: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

fn capture(label: &'static str) {
    OBSERVED.with(|o| o.borrow_mut().push(label));
}

#[test]
fn write_header_yields_before_fsync() {
    OBSERVED.with(|o| o.borrow_mut().clear());
    set_hook(Some(capture));

    sut.write_header().unwrap();

    OBSERVED.with(|o| {
        assert_eq!(*o.borrow(), vec!["write_header.before_fsync"]);
    });
    set_hook(None);
}
```

For fault injection (selective failure at a labeled point), match the label inside the callback and trip a flag the SUT then checks:

```rust
use std::sync::atomic::{AtomicBool, Ordering};

static INJECT_FAIL_AT_FSYNC: AtomicBool = AtomicBool::new(false);

fn fault(label: &'static str) {
    if label == "write_header.before_fsync"
        && INJECT_FAIL_AT_FSYNC.load(Ordering::Acquire)
    {
        panic!("simulated crash before fsync");
    }
}
```

### 6.4 Limitations

- **`yield_point!` in a `const fn`** — the runtime hook isn't callable in const context. rustc emits a downstream error; no custom macro-level diagnostic in v0.2.6.
- **No async-aware hook variant** — the callback runs on the calling thread; if the SUT is awaiting at the yield point, the hook fires when the future polls past it.
- **Per-thread state** — `set_hook` is thread-local. Multi-thread harnesses install the hook on each worker thread.

---

## 7. Feature gating — the consumer pattern

The canonical pattern is to wrap every macro invocation in `cfg_attr` keyed on a **project-local alias**, never `aristo_instrument` directly. This keeps your crate's surface uniform and lets you flip instrumentation on/off without editing call sites.

In your `Cargo.toml`:

```toml
[features]
default = []
# Your project-local feature name; aristo_instrument is the implementation detail.
differential-accessors = ["aristo/aristo_instrument"]
```

In your source:

```rust
#[cfg_attr(feature = "differential-accessors",
           derive(aristo::instrument::Inspect))]
pub struct Store { /* ... */ }

#[cfg_attr(feature = "differential-accessors",
           aristo::instrument::expose_pub(as = "new_for_test"))]
pub(crate) fn new(capacity: usize) -> Self { /* ... */ }

#[cfg(feature = "differential-accessors")]
aristo::instrument::yield_point!("commit.before_log_sync");
```

Building without the feature:
- The `cfg_attr` evaluates to nothing — the macro is never invoked.
- The original `pub(crate)` / unwrapped item is what compiles.
- The `yield_point!` call site is removed by the `#[cfg]`.

Building with the feature (`cargo build --features differential-accessors`):
- Aristo's `aristo_instrument` activates transitively.
- The macros expand.
- The harness side sees the raised visibility, the snapshot accessors, the yield point dispatches.

**Do not** put `aristo_instrument` directly in call sites. Doing so couples every consumer's `Cargo.toml` to aristo's exact flag name and removes the project-local naming layer that lets you pivot.

---

## 8. Quick decision tree

| You want to... | Reach for |
|---|---|
| Document a non-obvious correctness invariant on a function | `#[aristo::intent("...")]` |
| Document an invariant on a block / loop inside a fn body | `aristo::intent_stmt!("...")` |
| Document an environmental assumption (OS / library / caller) | `#[aristo::assume("...")]` (or `assume_stmt!`) |
| Snapshot a `SkipMap`'s entries for harness comparison | `#[derive(aristo::instrument::Inspect)]` + `#[inspect]` field tag |
| Snapshot the same with a projection / canonicalization | `#[inspect(T)]` with `impl From<&V> for T` |
| Snapshot a `BTreeMap` / `HashMap` / `Vec` / scalar | **Hand-write the accessor** (implementation debt; the macro errors for non-`SkipMap` fields in v0.2.6) |
| Expose a `pub(crate)` function to a harness across crate boundary | `#[expose_pub(as = "name_for_test")]` |
| Expose a `pub(crate)` type to a harness | `#[expose_pub]` on the type (no `as`) |
| Expose all methods of a `pub(crate)` impl block | `#[expose_pub]` on the `impl` (no `as`) |
| Insert a fault-injection point | `aristo::instrument::yield_point!("<fn>.<before/after>_<action>")` |
| Install a fault-injection callback in a test | `aristo::instrument::set_hook(Some(callback))` |
| Verify your annotations | `aristo verify --filter changed` (CLI; needs `cargo install aristo-cli`) |

---

## 9. Common pitfalls

1. **Forgetting the `cfg_attr` gate on instrument macros.** A bare `#[derive(Inspect)]` forces every downstream build to enable `aristo_instrument`. Always wrap in `#[cfg_attr(feature = "<your-alias>", ...)]`.

2. **Using clone mode (`#[inspect]`) on a non-`Clone` V.** Compile error from `V: Clone` requirement. Either derive `Clone` on V (if cheap) or switch to project mode with a per-field `From<&V> for Snapshot` impl that skips the un-cloneable internals.

3. **Trying `#[inspect]` on a `BTreeMap` / `HashMap` / `Vec` / scalar field.** The macro errors with `"only supports SkipMap<K, V> fields in v1"`. Hand-write the accessor for now; this is tracked as implementation debt, not a deferred-by-design constraint.

4. **`#[expose_pub(as = "...")]` on a type.** Forbidden — renaming a type would cascade through every reference. Use the bare `#[expose_pub]` form for types.

5. **`#[expose_pub]` on a fn without `as = "..."`.** Required — the wrapper needs a distinct name from the original.

6. **Vague `yield_point!` labels.** `"checkpoint"`, `"done"`, `"step"` — the harness selects by string match; vague labels make harness code ambiguous. Use `<fn-name>.before_<action>` consistently.

7. **`yield_point!` inside `const fn`.** The runtime hook is non-const; rustc emits a confusing error. Avoid; the macro is for runtime-context hooks.

8. **`verify` on `assume`.** Hard parse error with a hint to use `intent` instead. Assumptions describe what you rely on, not what to verify.

9. **`id` starting with `aret_` or `aristos:`.** Reserved tool-managed namespaces. Use plain snake_case.

10. **Trying to write back through an `inspect_X()` Vec.** It's an owned snapshot — there's no path back to the SUT. Mutate the SUT through its normal API; the snapshot is read-only by construction.

---

## 10. Cross-references

- **Conventions cheat-sheet (5 rules):** `docs/instrument-conventions.md`
- **Per-pattern recipes (cookbook):** `docs/instrument-recipes.md`
- **Architecture decision record:** `docs/decisions/instrument-surface.md`
- **Bundled authoring skill (instrument):** `crates/aristo-cli/src/skills/aristo-instrumenting.md`
- **Bundled authoring skill (intent):** `crates/aristo-cli/src/skills/aristo-authoring.md`
- **Roadmap (Phase 2 status + outstanding debt):** `docs/ROADMAP.md`
- **Manifesto (why aristo exists):** `docs/MANIFESTO.md`

This spec is the authoritative reference for v0.2.6. When the implementation debt closes (per-shape `Inspect` codegen) or new surface lands, this doc is updated alongside the release.
