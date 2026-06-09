//! Slice 40 integration test — the full `yield_point!` → runtime hook
//! round-trip. The macro expands to a call into `__yield_point`, which
//! dispatches to the thread-local hook installed via `set_hook`. With
//! no hook, the call is a silent no-op.

#![cfg(feature = "aristo_instrument")]

use aristo::instrument::{set_hook, yield_point};
use std::cell::RefCell;

thread_local! {
    static CAPTURED: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

fn capture(label: &'static str) {
    CAPTURED.with(|c| c.borrow_mut().push(label));
}

fn reset() {
    CAPTURED.with(|c| c.borrow_mut().clear());
    set_hook(None);
}

#[test]
fn no_hook_is_silent() {
    reset();
    yield_point!("silent.label");
    CAPTURED.with(|c| assert!(c.borrow().is_empty()));
}

#[test]
fn hook_receives_each_label_in_order() {
    reset();
    set_hook(Some(capture));

    yield_point!("first");
    yield_point!("second");
    yield_point!("third");

    CAPTURED.with(|c| {
        let labels = c.borrow();
        assert_eq!(*labels, vec!["first", "second", "third"]);
    });

    reset();
}

#[test]
fn replacing_hook_redirects_subsequent_calls() {
    reset();
    set_hook(Some(capture));
    yield_point!("captured_by_first_hook");

    // Install a noop hook that drops the label.
    fn drop_label(_: &'static str) {}
    set_hook(Some(drop_label));
    yield_point!("dropped_by_second_hook");

    // CAPTURED only has the first label; the second was swallowed by
    // the replacement hook.
    CAPTURED.with(|c| {
        let labels = c.borrow();
        assert_eq!(*labels, vec!["captured_by_first_hook"]);
    });

    reset();
}

#[test]
fn clearing_hook_restores_silence() {
    reset();
    set_hook(Some(capture));
    yield_point!("recorded");

    set_hook(None);
    yield_point!("not_recorded");

    CAPTURED.with(|c| {
        let labels = c.borrow();
        assert_eq!(*labels, vec!["recorded"]);
    });
}
