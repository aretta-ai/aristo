//! Smoke test: the `aristo` binary builds, is locatable via `assert_cmd`, and
//! produces the documented stub behavior. This test is the canary for the
//! test harness wiring itself — if it fails, the trycmd scenarios won't be
//! reachable either. It is rewritten in the slice that lands real CLI
//! dispatch (the stub stderr disappears at that point).

use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn stub_binary_exits_with_unimplemented_message() {
    Command::cargo_bin("aristo")
        .unwrap()
        .assert()
        .failure()
        .code(1)
        .stderr(contains("not yet implemented"));
}
