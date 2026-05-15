//! `trybuild` compile-pass UI tests for slice 7.
//!
//! Each `.rs` file under `tests/ui/pass/` is compiled as a standalone
//! program. The test passes if every file compiles cleanly. Compile-fail
//! cases (with locked `.stderr` snapshots) land with the `aristo_check`
//! cargo feature in slice 8.
//!
//! Why trybuild: it isolates each fixture in its own `cargo build`, so a
//! failure in one fixture doesn't mask others. The fixtures double as
//! executable mockup-01 examples — a reader can paste any of them into
//! their own crate and see the macros work.

#[test]
fn compile_pass() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass/*.rs");
}
