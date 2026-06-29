//! Bare `#[inspect]` (clone mode) on a non-`Clone` field. The deferred
//! `Clone` bound must fail on the offending FIELD, not on the whole
//! `#[derive(Inspect)]` — so the author sees which field to switch to
//! projection mode. Regression guard for aretta-bench finding C4.
use aristo::instrument::Inspect;

struct NoClone;

#[derive(Inspect)]
struct S {
    #[inspect]
    field: NoClone,
}

fn main() {}
