//! `aristo metrics` — JSON contract + human-summary integration tests.

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

fn write_lib(root: &Path, content: &str) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), content).unwrap();
}

#[test]
fn metrics_json_emits_a_parseable_metrics_payload() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"
            #[aristo::intent("one", verify = "test", id = "a")] fn a() {}
            #[aristo::intent("two", verify = "neural", id = "b")] fn b() {}
            #[aristo::intent("doc only", verify = false, id = "c")] fn c() {}
            #[aristo::assume("external invariant", id = "d")] fn d() {}
        "#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();

    let out = aristo_in(tmp.path())
        .args(["metrics", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "metrics --json must exit 0");
    let json = String::from_utf8(out.stdout).unwrap();

    // The frozen contract: stdout parses straight into the core Metrics type.
    let m: aristo_core::metrics::Metrics =
        serde_json::from_str(&json).expect("metrics --json must parse into aristo_core Metrics");
    assert_eq!(m.intents, 3, "3 intents (the assume is excluded)");
    assert_eq!(m.assumes, 1);
    assert_eq!(m.verifiable, 2, "verify=false is not verifiable");
    assert_eq!(m.verified_clean, 0, "nothing verified just after stamp");
    assert_eq!(m.unverified, 2);
    assert_eq!(
        m.schema_version,
        aristo_core::metrics::METRICS_SCHEMA_VERSION
    );
}

#[test]
fn metrics_human_summary_lists_counts_and_tier() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("one", verify = "test", id = "a")] fn a() {}"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    aristo_in(tmp.path())
        .arg("metrics")
        .assert()
        .success()
        .stdout(contains("Aristo metrics"))
        .stdout(contains("intents:"))
        .stdout(contains("unverified:"))
        .stdout(contains("tier:"));
}
