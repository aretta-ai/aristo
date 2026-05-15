//! Behavior-driven CLI tests, sourced from the Phase 0 mockups in
//! `../../../docs/mockups/`. Scenarios live in `tests/cmd/` as Markdown files
//! with `console`-fenced command blocks (trycmd format).
//!
//! ### Directory convention
//!
//! - `tests/cmd/active/` — scenarios for commands that are implemented;
//!   trycmd runs these and they MUST pass.
//! - `tests/cmd/_pending/` — scenarios for commands not yet implemented.
//!   Deliberately not picked up by the glob below; visible documentation
//!   of unimplemented surface.
//!
//! ### Lifecycle of a scenario
//!
//! 1. New mockup → write scenario file in `tests/cmd/_pending/<name>.md`.
//! 2. Implementing the command → `mv` the file into `tests/cmd/active/`
//!    in the SAME commit that lands the implementation.
//! 3. Behavior change → update the scenario in the same commit as the code.
//!
//! See `docs/TESTING.md` for full conventions.

#[test]
fn cli_scenarios() {
    trycmd::TestCases::new().case("tests/cmd/active/*.md");
}
