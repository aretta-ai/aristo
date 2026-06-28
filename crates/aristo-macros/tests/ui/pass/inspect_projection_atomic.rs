//! Projection mode over a NON-`Clone` field (`AtomicU64`). Clone mode
//! could not express this (atomics are not `Clone`); the projector loads
//! the atomic into an owned snapshot.

use aristo::instrument::Inspect;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Inspect)]
struct Clock {
    #[inspect(ret = u64, with = |a| a.load(Ordering::Acquire))]
    ticks: AtomicU64,
}

fn main() {
    let c = Clock {
        ticks: AtomicU64::new(42),
    };
    assert_eq!(c.inspect_ticks(), 42u64);
}
