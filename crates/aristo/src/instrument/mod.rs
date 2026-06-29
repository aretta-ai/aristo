//! `aristo::instrument` — proc-macro re-exports + runtime hook.
//!
//! The three proc-macros (`Inspect` derive, `expose_pub` attribute,
//! `yield_point!` function-like) live in `aristo-macros` and are
//! re-exported here so users import them via the umbrella crate.
//!
//! The runtime hook (`set_hook` + `__yield_point`) is defined inline.
//! A `proc-macro = true` crate can't export non-macro items, and
//! `aristo-core` already depends on this meta-crate (for the `intent` /
//! `assume` macros it dogfoods), so the cleanest home for the ~50-line
//! runtime piece is right here in `aristo`. Slice 36 ships the hook
//! complete; the `yield_point!` macro that calls it lands in slice 40.

pub use aristo_macros::{expose_pub, fault_point, yield_point, Inspect};

use std::cell::Cell;

thread_local! {
    // `Option<fn(&'static str)>` rather than `Option<Box<dyn Fn>>` so the
    // hot path is allocation-free: a `yield_point!` call expands to one
    // load + one branch + one indirect call when a hook is set, and one
    // load + one branch when not. Labels are always `'static` because
    // the macro requires a string literal at the call site.
    static HOOK: Cell<Option<fn(&'static str)>> = const { Cell::new(None) };
}

/// Install a thread-local callback invoked by every `yield_point!`
/// expansion (when the `aristo_instrument` feature is on). Passing
/// `None` clears any previously installed hook. Calling `set_hook`
/// replaces the prior hook for the current thread — no chaining.
pub fn set_hook(hook: Option<fn(&'static str)>) {
    HOOK.with(|h| h.set(hook));
}

/// Internal entry point invoked by the `yield_point!` macro expansion.
/// Not part of the stable user surface — call `yield_point!("label")`
/// from user code; this function is the dispatch target.
#[doc(hidden)]
pub fn __yield_point(label: &'static str) {
    if let Some(hook) = HOOK.with(|h| h.get()) {
        hook(label);
    }
}

/// The decision a `fault_point!` relays from the harness to the SUT.
///
/// `aristo` assigns **no meaning** to `Inject` — the `u64` is an opaque code
/// the SUT decodes into its own effect (an errno, a short-write prefix length,
/// a timeout, a drop count). The instrument surface stays fault-agnostic: it
/// carries *when* to inject plus an opaque selector; the SUT owns *what* the
/// fault is and performs it. `#[non_exhaustive]` so a richer variant can be
/// added later without a breaking change.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// No fault — proceed normally. The default when no fault hook is set.
    Continue,
    /// The harness asked the SUT to inject a fault here; the SUT decodes the
    /// opaque code. `aristo` never interprets it.
    Inject(u64),
}

/// A fault policy: a capturing closure mapping a `fault_point!` label to a
/// [`Decision`]. Boxed because it captures the harness's per-scenario state
/// (e.g. a "fail the Nth" counter); `FnMut` because that state mutates.
pub type FaultHook = Box<dyn FnMut(&'static str) -> Decision>;

thread_local! {
    // A *capturing* `FnMut` (vs the non-capturing `fn` of `HOOK`) so a harness
    // policy can own per-scenario state — the invocation counter for "fail the
    // Nth" — without a process-global `static`. One `Box` alloc at
    // `set_fault_hook` time; dispatch is take-call-restore, so the cell is
    // never borrowed across the closure and a re-entrant `fault_point!` cannot
    // double-borrow-panic.
    static FAULT_HOOK: Cell<Option<FaultHook>> = const { Cell::new(None) };
}

/// Install a thread-local fault policy invoked by every `fault_point!`
/// expansion (when the `aristo_instrument` feature is on). The closure
/// **captures**, so the harness's counter / chosen error lives inside it.
/// `None` clears the policy; a fresh call replaces the prior one (no chaining).
pub fn set_fault_hook(hook: Option<FaultHook>) {
    FAULT_HOOK.with(|h| h.set(hook));
}

/// Internal entry point invoked by the `fault_point!` macro expansion. Returns
/// the policy's `Decision`, or `Continue` when no policy is installed.
///
/// Dispatch is **take-call-restore**: the closure is moved out of the cell
/// before it is called, so a `fault_point!` reached re-entrantly *inside* the
/// policy sees an empty cell and returns `Continue` instead of panicking. If
/// the policy installs a new hook mid-call, the new one wins.
#[doc(hidden)]
pub fn __fault_point(label: &'static str) -> Decision {
    let Some(mut hook) = FAULT_HOOK.with(|h| h.take()) else {
        return Decision::Continue;
    };
    let decision = hook(label);
    FAULT_HOOK.with(|h| match h.take() {
        // The policy re-installed a hook mid-call — keep it, drop the original.
        Some(reinstalled) => h.set(Some(reinstalled)),
        // Normal case — restore the original policy.
        None => h.set(Some(hook)),
    });
    decision
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    thread_local! {
        static OBSERVED: Cell<Option<&'static str>> = const { Cell::new(None) };
    }

    fn capture(label: &'static str) {
        OBSERVED.with(|o| o.set(Some(label)));
    }

    fn ignore(_: &'static str) {}

    #[test]
    fn default_is_noop() {
        set_hook(None);
        __yield_point("noop.label");
        // No panic, no observable effect: the hook is None.
    }

    #[test]
    fn hook_receives_label() {
        OBSERVED.with(|o| o.set(None));
        set_hook(Some(capture));
        __yield_point("captured.label");
        assert_eq!(OBSERVED.with(|o| o.get()), Some("captured.label"));
        set_hook(None);
    }

    #[test]
    fn replacing_hook_invokes_only_the_latest() {
        OBSERVED.with(|o| o.set(None));
        set_hook(Some(capture));
        set_hook(Some(ignore));
        __yield_point("ignored.label");
        // The first hook is gone — OBSERVED stays None.
        assert_eq!(OBSERVED.with(|o| o.get()), None);
        set_hook(None);
    }

    #[test]
    fn clearing_hook_restores_noop() {
        OBSERVED.with(|o| o.set(None));
        set_hook(Some(capture));
        set_hook(None);
        __yield_point("after.clear");
        assert_eq!(OBSERVED.with(|o| o.get()), None);
    }

    // --- fault_point! dispatch -------------------------------------------

    #[test]
    fn fault_default_is_continue() {
        set_fault_hook(None);
        assert_eq!(__fault_point("no.hook"), Decision::Continue);
    }

    #[test]
    fn fault_hook_fails_nth() {
        // The capturing closure owns the invocation counter — the whole
        // reason a non-capturing `fn` is insufficient for "fail the Nth".
        let mut seen = 0u64;
        set_fault_hook(Some(Box::new(move |_label| {
            seen += 1;
            if seen == 2 {
                Decision::Inject(7)
            } else {
                Decision::Continue
            }
        })));
        assert_eq!(__fault_point("op"), Decision::Continue);
        assert_eq!(__fault_point("op"), Decision::Inject(7));
        assert_eq!(__fault_point("op"), Decision::Continue);
        set_fault_hook(None);
    }

    #[test]
    fn replacing_fault_hook_invokes_only_the_latest() {
        set_fault_hook(Some(Box::new(|_| Decision::Inject(1))));
        set_fault_hook(Some(Box::new(|_| Decision::Continue)));
        assert_eq!(__fault_point("op"), Decision::Continue);
        set_fault_hook(None);
    }

    #[test]
    fn clearing_fault_hook_restores_continue() {
        set_fault_hook(Some(Box::new(|_| Decision::Inject(9))));
        set_fault_hook(None);
        assert_eq!(__fault_point("op"), Decision::Continue);
    }

    #[test]
    fn reentrant_fault_point_is_continue_and_hook_restored() {
        // A policy that itself reaches a fault_point must not panic: the
        // closure is taken out for the duration, so the nested call sees no
        // hook and returns Continue. The hook is restored afterwards.
        set_fault_hook(Some(Box::new(|_| {
            assert_eq!(__fault_point("inner"), Decision::Continue);
            Decision::Inject(3)
        })));
        assert_eq!(__fault_point("outer"), Decision::Inject(3));
        // Still installed for the next call (restore happened).
        assert_eq!(__fault_point("outer"), Decision::Inject(3));
        set_fault_hook(None);
    }
}
