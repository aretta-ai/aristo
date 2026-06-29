//! `fault_point!` returns a `Decision` the SUT branches on to inject a fault;
//! the harness installs a capturing policy. aretta-bench fault-injection
//! primitive (additive — `yield_point!` / `set_hook` are untouched).
use aristo::instrument::{fault_point, set_fault_hook, Decision};

// A SUT operation with an interior fault point: on `Inject` it surfaces the
// harness-supplied opaque code as its own error.
fn commit() -> Result<(), u64> {
    if let Decision::Inject(code) = fault_point!("txn.before_commit") {
        return Err(code);
    }
    Ok(())
}

fn main() {
    // Harness: a capturing policy that fails the 2nd commit with code 5 — the
    // counter lives in the closure, no process-global static.
    let mut n = 0u64;
    set_fault_hook(Some(Box::new(move |_label| {
        n += 1;
        if n == 2 {
            Decision::Inject(5)
        } else {
            Decision::Continue
        }
    })));
    let _ = commit();
    set_fault_hook(None);
}
