//! `aristo lint --check` — imperative integration tests for the rule
//! catalog and exit-code semantics.
//!
//! `aristo lint` reads the workspace's `.aristo/index.toml` and runs
//! the built-in rule catalog against each entry's annotation text. No
//! source-walking (that's --fix's job in C2). No LLM. No network.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::fs;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

/// Build a workspace + index with one intent having the given text. No
/// stamp — we hand-craft the index so tests don't depend on the walker.
fn workspace_with_one_intent(
    dir: &Path,
    id: &str,
    text: &str,
    file: &str,
    severity_overrides: &[(&str, &str)],
) {
    aristo_in(dir).arg("init").assert().success();

    let mut config = String::new();
    if !severity_overrides.is_empty() {
        config.push_str("[lint]\n");
        for (rule, sev) in severity_overrides {
            config.push_str(&format!("[lint.rules.{rule}]\nseverity = \"{sev}\"\n"));
        }
        fs::write(dir.join("aristo.toml"), config).unwrap();
    }

    let zero_hash = format!("sha256:{}", "0".repeat(64));
    let index = format!(
        "[__meta__]\nschema_version = 1\n\n\
         [{id}]\nkind = \"intent\"\ntext = \"{text}\"\nverify = \"test\"\nstatus = \"unknown\"\n\
         text_hash = \"{zero_hash}\"\nbody_hash = \"{zero_hash}\"\n\
         file = \"{file}\"\nsite = \"fn x (line 1)\"\ncovered_region = \"function\"\n",
    );
    fs::write(dir.join(".aristo/index.toml"), index).unwrap();
}

#[test]
fn errors_outside_a_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path())
        .args(["lint", "--check"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("not inside an Aristo workspace"));
}

#[test]
fn empty_text_is_error_severity_and_fails_check() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_intent(tmp.path(), "cache_eviction_pre", "", "src/cache.rs", &[]);

    aristo_in(tmp.path())
        .args(["lint", "--check"])
        .assert()
        .failure()
        .code(1)
        .stdout(contains("cache_eviction_pre"))
        .stdout(contains("empty_text"));
}

#[test]
fn clean_text_passes_check_with_zero_findings() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_intent(
        tmp.path(),
        "balance_no_dups",
        "Balance never duplicates cells.",
        "src/btree.rs",
        &[],
    );

    aristo_in(tmp.path())
        .args(["lint", "--check"])
        .assert()
        .success()
        .stdout(contains("0 lint finding").or(contains("ok:")));
}

#[test]
fn weasel_words_are_warn_and_do_not_fail_check_by_default() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_intent(
        tmp.path(),
        "x",
        "This function should probably handle the edge case.",
        "src/x.rs",
        &[],
    );

    // `should` and `probably` are weasels; severity=warn; --check exits 0.
    aristo_in(tmp.path())
        .args(["lint", "--check"])
        .assert()
        .success();
}

#[test]
fn weasel_words_fail_check_under_strict() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_intent(
        tmp.path(),
        "x",
        "This function should probably handle it.",
        "src/x.rs",
        &[],
    );

    aristo_in(tmp.path())
        .args(["lint", "--check", "--strict"])
        .assert()
        .failure()
        .code(1)
        .stdout(contains("weasel_words"));
}

#[test]
fn placeholder_text_is_error_severity() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_intent(
        tmp.path(),
        "x",
        "TODO: write a real description here.",
        "src/x.rs",
        &[],
    );

    aristo_in(tmp.path())
        .args(["lint", "--check"])
        .assert()
        .failure()
        .code(1)
        .stdout(contains("placeholder_text"));
}

#[test]
fn text_too_long_is_warn_and_only_fails_under_strict() {
    let tmp = tempfile::tempdir().unwrap();
    let long = "a ".repeat(550); // > 1000 chars after joining
    workspace_with_one_intent(tmp.path(), "x", &long, "src/x.rs", &[]);

    aristo_in(tmp.path())
        .args(["lint", "--check"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["lint", "--check", "--strict"])
        .assert()
        .failure()
        .stdout(contains("text_too_long"));
}

#[test]
fn per_rule_severity_override_in_aristo_toml_takes_effect() {
    let tmp = tempfile::tempdir().unwrap();
    // Override placeholder_text from error -> info; should now pass --check.
    workspace_with_one_intent(
        tmp.path(),
        "x",
        "TODO: real text later.",
        "src/x.rs",
        &[("placeholder_text", "info")],
    );

    aristo_in(tmp.path())
        .args(["lint", "--check"])
        .assert()
        .success();
}

// ─── --fix tests ────────────────────────────────────────────────────────

#[test]
fn fix_errors_outside_a_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path())
        .args(["lint", "--fix"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("not inside an Aristo workspace"));
}

#[test]
fn fix_clean_workspace_reports_zero_fixes_zero_files() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("src/lib.rs"),
        "#[aristo::intent(\"already clean\")]\nfn x() {}\n",
    )
    .unwrap();

    aristo_in(tmp.path())
        .args(["lint", "--fix"])
        .assert()
        .success()
        .stdout(contains("fixed: 0 whitespace issues across 0 files"));
}

#[test]
fn fix_rewrites_source_in_place_with_correct_count() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    let path = tmp.path().join("src/lib.rs");
    fs::write(
        &path,
        "#[aristo::intent(\"text with  trailing whitespace  \")]\nfn x() {}\n",
    )
    .unwrap();

    aristo_in(tmp.path())
        .args(["lint", "--fix"])
        .assert()
        .success()
        .stdout(contains("fixed: 2 whitespace issues across 1 file"));

    let after = fs::read_to_string(&path).unwrap();
    assert!(
        after.contains("\"text with trailing whitespace\""),
        "expected normalized text in file; got: {after}"
    );
}

#[test]
fn fix_ignores_non_aristo_string_literals() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    let path = tmp.path().join("src/lib.rs");
    // The let-binding has doubled spaces + trailing whitespace inside a
    // plain string literal; the lint should not touch it.
    fs::write(&path, "fn x() {\n    let _ = \"unrelated  text  \";\n}\n").unwrap();

    aristo_in(tmp.path())
        .args(["lint", "--fix"])
        .assert()
        .success()
        .stdout(contains("fixed: 0 whitespace issues across 0 files"));

    let after = fs::read_to_string(&path).unwrap();
    assert!(
        after.contains("\"unrelated  text  \""),
        "non-aristo literal must be untouched; got: {after}"
    );
}

#[test]
fn multiple_findings_listed_in_stable_order() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();

    let zero_hash = format!("sha256:{}", "0".repeat(64));
    let index = format!(
        "[__meta__]\nschema_version = 1\n\n\
         [alpha]\nkind = \"intent\"\ntext = \"\"\nverify = \"test\"\nstatus = \"unknown\"\n\
         text_hash = \"{zero_hash}\"\nbody_hash = \"{zero_hash}\"\n\
         file = \"src/a.rs\"\nsite = \"fn a (line 1)\"\ncovered_region = \"function\"\n\n\
         [bravo]\nkind = \"intent\"\ntext = \"TODO\"\nverify = \"test\"\nstatus = \"unknown\"\n\
         text_hash = \"{zero_hash}\"\nbody_hash = \"{zero_hash}\"\n\
         file = \"src/b.rs\"\nsite = \"fn b (line 1)\"\ncovered_region = \"function\"\n",
    );
    fs::write(tmp.path().join(".aristo/index.toml"), index).unwrap();

    let assert = aristo_in(tmp.path())
        .args(["lint", "--check"])
        .assert()
        .failure()
        .code(1);
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let alpha = stdout.find("alpha").unwrap();
    let bravo = stdout.find("bravo").unwrap();
    assert!(
        alpha < bravo,
        "findings should be sorted by id (alpha before bravo); got:\n{stdout}"
    );
}
