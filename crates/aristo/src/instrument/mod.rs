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

pub use aristo_macros::{expose_pub, yield_point, Inspect};

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
}
