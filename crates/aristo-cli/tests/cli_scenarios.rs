//! Behavior-driven CLI tests, sourced from the Phase 0 mockups in
//! `../../../docs/mockups/`. Scenarios live in `tests/cmd/` as Markdown files
//! with `console`-fenced command blocks (trycmd format).
//!
//! ### Directory convention (per CLAUDE.md §12 — specifications are the truth)
//!
//! - `tests/cmd/active/` — scenarios for shipped behavior; trycmd runs
//!   these and they MUST pass byte-for-byte (modulo trycmd wildcards).
//! - `tests/cmd/_pending/` — spec for features whose implementation has
//!   not been scoped to a slice yet (Phase 2 server commands, paid-tier
//!   verification, etc.). Not picked up by the glob below.
//! - `tests/cmd/_blocked/` — spec for features whose implementation is
//!   committed to a specific upcoming slice (e.g. slice 21 for the
//!   pre-commit hook). Not picked up by the glob below; tracked in
//!   `_blocked/README.md` with the unblocking slice for each scenario.
//!
//! ### Lifecycle of a scenario
//!
//! 1. New mockup → write scenario file in `tests/cmd/_pending/<name>.md`
//!    or `_blocked/<name>.md` depending on whether a target slice is
//!    committed.
//! 2. Implementing the command → `mv` the file into `tests/cmd/active/`
//!    BYTE-FOR-BYTE in the SAME commit that lands the implementation.
//!    Per §12: the spec is the truth; never edit the scenario to fit
//!    the impl. If the scenario doesn't pass after promotion, fix the
//!    impl, not the spec.
//! 3. Behavior change → if the spec changes, that's a design amendment
//!    requiring human signoff (per §12); update the spec FIRST in the
//!    design archive (`../aretta-sdk/docs/`), then propagate to the
//!    scenario.
//!
//! See `docs/TESTING.md` for full conventions and `_blocked/README.md`
//! for the deferred-spec tracking.

#[test]
fn cli_scenarios() {
    // Isolate spawned binaries from developer-machine credentials.
    // Without this, a developer who has logged in to dev.aretta.ai
    // via `aristo auth login --server dev` sees `lifecycle_ci_gates.md`
    // fail: their real credentials at
    // `$HOME/Library/Application Support/aristo/credentials` (macOS) or
    // `$XDG_CONFIG_HOME/aristo/credentials` leak into trycmd's spawned
    // `aristo` process via inherited HOME, flipping `aristo status`'s
    // canon-binding block from "no token (free-tier mode)" (the spec)
    // to "authenticated (token present)". The token never reaches the
    // wire — it just changes the local rendering — but the byte-exact
    // trycmd assertion sees the difference.
    //
    // The auth E2E tests (`auth_oauth_login.rs`) use the same pattern
    // per-Command via `c.env_clear()`. Here the spawn happens inside
    // trycmd, so we set the env defaults at the TestCases level
    // instead. `TestCases::env` was added in trycmd 0.15.
    //
    // The tempdir lives until end of scope; `TestCases::case` doesn't
    // run the cases — they run in `TestCases::drop`, which fires
    // before `_isolated_home` drops (reverse declaration order).
    let _isolated_home = tempfile::tempdir().expect("isolated HOME for cli_scenarios");
    let home = _isolated_home.path().display().to_string();
    let xdg = _isolated_home.path().join("xdg").display().to_string();
    let cases = trycmd::TestCases::new();
    cases.env("HOME", home);
    cases.env("XDG_CONFIG_HOME", xdg);
    cases.case("tests/cmd/active/*.md");
}
