//! `aristo lang` — imperative integration tests for the surfaces trycmd
//! doesn't easily exercise (the `--file` flag's per-extension dispatch).

use assert_cmd::Command;
use predicates::str::contains;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

#[test]
fn file_flag_with_rs_extension_succeeds_with_rust_cheat_sheet() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path())
        .args(["lang", "--file", "src/lib.rs"])
        .assert()
        .success()
        .stdout(contains("Detected language: Rust"))
        .stdout(contains("aristo::intent_stmt!"));
}

#[test]
fn file_flag_with_unsupported_extension_errors_with_planned_list() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path())
        .args(["lang", "--file", "scripts/setup.py"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Cannot detect a supported language"))
        .stderr(contains("Aristo supports: Rust"))
        .stderr(contains("Planned: Python, Go, TypeScript"));
}

#[test]
fn no_args_in_dir_with_cargo_toml_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    aristo_in(tmp.path())
        .arg("lang")
        .assert()
        .success()
        .stdout(contains("Detected language: Rust"))
        .stdout(contains("Cargo.toml"));
}

#[test]
fn no_args_in_dir_without_cargo_toml_errors() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path())
        .arg("lang")
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Cannot detect a supported language"));
}

#[test]
fn cheat_sheet_uses_intent_stmt_not_intent_bang() {
    // Regression guard: the cheat sheet must reference the macro names we
    // actually export. Slice 6 ships `intent_stmt!`, NOT `intent!` (E0428
    // forces distinct names per kind).
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("Cargo.toml"), "").unwrap();
    let out = aristo_in(tmp.path()).arg("lang").output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("aristo::intent_stmt!"),
        "cheat sheet must reference intent_stmt! (the actual exported name); got:\n{stdout}"
    );
    assert!(
        !stdout.contains("aristo::intent!("),
        "cheat sheet must NOT reference intent!() (that name is reserved \
         for the attribute form per E0428); got:\n{stdout}"
    );
}
