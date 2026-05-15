//! `trybuild` UI tests.
//!
//! Each `.rs` file under `tests/ui/pass/` must compile cleanly; each under
//! `tests/ui/fail/` must fail compilation with the exact error in the
//! sibling `.stderr` snapshot.
//!
//! Why trybuild: it isolates each fixture in its own `cargo build`, so a
//! failure in one fixture doesn't mask others. The pass fixtures double as
//! executable mockup-01 examples — a reader can paste any of them into
//! their own crate and see the macros work; the fail fixtures double as
//! the user-facing diagnostic catalog for `aristo_check`.
//!
//! Re-snapshotting after intentional message changes:
//!
//! ```text
//! TRYBUILD=overwrite cargo test -p aristo-macros --test trybuild_compile_pass
//! ```

#[test]
fn compile_pass() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass/*.rs");
}

#[test]
fn compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/fail/*.rs");
}
