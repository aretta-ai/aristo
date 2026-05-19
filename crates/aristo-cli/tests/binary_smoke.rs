//! Smoke test: the `aristo` binary's CLI dispatch layer wires up correctly.
//!
//! These tests spawn the real binary (unlike the unit tests in `lib.rs`,
//! which call `dispatch` directly). They are the canary for `main.rs` ↔
//! `lib::run()` glue, clap's argument parsing at the binary boundary, and
//! exit-code propagation through `ExitCode`. If these fail, every other
//! integration test stops being meaningful.

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;

#[test]
fn no_subcommand_shows_clap_usage_with_exit_2() {
    // clap convention: missing required subcommand exits 2.
    Command::cargo_bin("aristo")
        .unwrap()
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Usage:").or(contains("usage:")));
}

#[test]
fn version_flag_succeeds() {
    Command::cargo_bin("aristo")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("aristo"));
}

#[test]
fn help_flag_lists_offline_subcommands() {
    let assert = Command::cargo_bin("aristo")
        .unwrap()
        .arg("--help")
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("help is utf-8");
    // Spot-check a few subcommands — the full list is locked by the unit
    // test in `lib::tests::every_subcommand_dispatches_to_a_distinct_slice`.
    for cmd in ["init", "stamp", "show", "list", "verify", "doc", "graph"] {
        assert!(
            stdout.contains(cmd),
            "expected `{cmd}` in --help; got:\n{stdout}"
        );
    }
}

#[test]
fn unknown_subcommand_argument_rejected_with_exit_2() {
    // After slice 32 shipped, every defined subcommand has a real
    // implementation — there are no more `not_yet(...)` stubs in
    // dispatch. This canary keeps the binary's argv-parser wired to
    // clap's exit-2 misuse path: passing a bogus flag to a real
    // command still surfaces a structured clap error, not a panic.
    Command::cargo_bin("aristo")
        .unwrap()
        .args(["rename", "--not-a-real-flag", "foo", "bar"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("unexpected argument").or(contains("--not-a-real-flag")));
}

#[test]
fn unknown_subcommand_rejected_by_clap_with_exit_2() {
    Command::cargo_bin("aristo")
        .unwrap()
        .arg("frobnicate")
        .assert()
        .failure()
        .code(2)
        .stderr(contains("frobnicate").or(contains("unrecognized")));
}
