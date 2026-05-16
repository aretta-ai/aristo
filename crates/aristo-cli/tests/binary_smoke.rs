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
fn defined_but_unimplemented_subcommand_exits_64() {
    // `aristo show` is defined; its body is the stub from slice 9 (slice
    // 18 will replace it). The exit code (64, "EX_USAGE-ish, not yet
    // implemented") and the stderr message tell the user when to expect
    // the real implementation.
    //
    // When slice 18 lands and `show` becomes a real command, swap to any
    // other still-stubbed variant — `list`, `status`, etc.
    Command::cargo_bin("aristo")
        .unwrap()
        .arg("show")
        .assert()
        .failure()
        .code(64)
        .stderr(contains("not yet implemented"))
        .stderr(contains("slice 18"));
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
